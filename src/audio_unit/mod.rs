//! This module is an attempt to provide a friendly, rust-esque interface to Apple's Audio Unit API.
//!
//! Learn more about the Audio Unit API [here](https://developer.apple.com/library/mac/documentation/MusicAudio/Conceptual/AudioUnitProgrammingGuide/Introduction/Introduction.html#//apple_ref/doc/uid/TP40003278-CH1-SW2)
//! and [here](https://developer.apple.com/library/mac/documentation/MusicAudio/Conceptual/AudioUnitProgrammingGuide/TheAudioUnit/TheAudioUnit.html).
//!
//! TODO: The following are `kAudioUnitSubType`s (along with their const u32) generated by
//! rust-bindgen that we could not find any documentation on:
//!
//! - MIDISynth            = 1836284270,
//! - RoundTripAAC         = 1918984547,
//! - SpatialMixer         = 862217581,
//! - SphericalHeadPanner  = 1936746610,
//! - VectorPanner         = 1986158963,
//! - SoundFieldPanner     = 1634558569,
//! - HRTFPanner           = 1752331366,
//! - NetReceive           = 1852990326,
//!
//! If you can find documentation on these, please feel free to submit an issue or PR with the
//! fixes!


use bindings::audio_unit as au;
use error::Error;
use libc;
use std::mem;
use std::ptr;

pub use self::audio_format::AudioFormat;
pub use self::stream_format::StreamFormat;
pub use self::types::{
    Type,
    EffectType,
    FormatConverterType,
    GeneratorType,
    IOType,
    MixerType,
    MusicDeviceType,
};


pub mod audio_format;
pub mod render_callback;
pub mod stream_format;
pub mod types;


/// The input and output **Scope**s.
///
/// More info [here](https://developer.apple.com/library/ios/documentation/AudioUnit/Reference/AudioUnitPropertiesReference/index.html#//apple_ref/doc/constant_group/Audio_Unit_Scopes)
/// and [here](https://developer.apple.com/library/mac/documentation/MusicAudio/Conceptual/AudioUnitProgrammingGuide/TheAudioUnit/TheAudioUnit.html).
#[derive(Copy, Clone, Debug)]
pub enum Scope {
    Output = 0,
    Input  = 1,
}

/// Represents the **Input** and **Output** **Element**s.
///
/// These are used when specifying which **Element** we're setting the properties of.
#[derive(Copy, Clone, Debug)]
pub enum Element {
    Output = 0,
    Input  = 1,
}


/// A rust representation of the au::AudioUnit, including a pointer to the current rendering callback.
///
/// Find the original Audio Unit Programming Guide [here](https://developer.apple.com/library/mac/documentation/MusicAudio/Conceptual/AudioUnitProgrammingGuide/TheAudioUnit/TheAudioUnit.html).
pub struct AudioUnit {
    instance: au::AudioUnit,
    maybe_callback: Option<*mut render_callback::InputProcFnWrapper>
}


macro_rules! try_os_status {
    ($expr:expr) => (try!(Error::from_os_status($expr)))
}


impl AudioUnit {

    /// Construct a new AudioUnit with any type that may be automatically converted into
    /// [**Type**](./enum.Type).
    ///
    /// Here is a list of compatible types:
    ///
    /// - [**Type**](./types/enum.Type)
    /// - [**IOType**](./types/enum.IOType)
    /// - [**MusicDeviceType**](./types/enum.MusicDeviceType)
    /// - [**GeneratorType**](./types/enum.GeneratorType)
    /// - [**FormatConverterType**](./types/enum.FormatConverterType)
    /// - [**EffectType**](./types/enum.EffectType)
    /// - [**MixerType**](./types/enum.MixerType)
    ///
    /// To construct the **AudioUnit** with some component flags, see
    /// [**AudioUnit::new_with_flags**](./struct.AudioUnit#method.new_with_flags).
    ///
    /// Note: the `AudioUnit` is constructed with the `kAudioUnitManufacturer_Apple` Manufacturer
    /// Identifier, as this is the only Audio Unit Manufacturer Identifier documented by Apple in
    /// the AudioUnit reference (see [here](https://developer.apple.com/library/prerelease/mac/documentation/AudioUnit/Reference/AUComponentServicesReference/index.html#//apple_ref/doc/constant_group/Audio_Unit_Manufacturer_Identifier)).
    pub fn new<T>(ty: T) -> Result<AudioUnit, Error>
        where T: Into<Type>,
    {
        AudioUnit::new_with_flags(ty, 0, 0)
    }

    /// The same as [**AudioUnit::new**](./struct.AudioUnit#method.new) but with the given
    /// component flags and mask.
    pub fn new_with_flags<T>(ty: T, flags: u32, mask: u32) -> Result<AudioUnit, Error>
        where T: Into<Type>,
    {
        const MANUFACTURER_IDENTIFIER: u32 = au::kAudioUnitManufacturer_Apple;
        let au_type: Type = ty.into();
        let sub_type_u32 = match au_type.to_subtype_u32() {
            Some(u) => u,
            None => return Err(Error::NoKnownSubtype),
        };

        // A description of the audio unit we desire.
        let desc = au::AudioComponentDescription {
            componentType: au_type.to_u32() as libc::c_uint,
            componentSubType: sub_type_u32 as libc::c_uint,
            componentManufacturer: MANUFACTURER_IDENTIFIER,
            componentFlags: flags,
            componentFlagsMask: mask,
        };

        unsafe {
            // Find the default audio unit for the description.
            //
            // From the "Audio Unit Hosting Guide for iOS":
            //
            // Passing NULL to the first parameter of AudioComponentFindNext tells this function to
            // find the first system audio unit matching the description, using a system-defined
            // ordering. If you instead pass a previously found audio unit reference in this
            // parameter, the function locates the next audio unit matching the description.
            let component = au::AudioComponentFindNext(ptr::null_mut(), &desc as *const _);
            if component.is_null() {
                return Err(Error::NoMatchingDefaultAudioUnitFound);
            }

            // Create an instance of the default audio unit using the component.
            let mut instance: au::AudioUnit = mem::uninitialized();
            try_os_status!(
                au::AudioComponentInstanceNew(component, &mut instance as *mut au::AudioUnit)
            );

            // Initialise the audio unit!
            try_os_status!(au::AudioUnitInitialize(instance));
            Ok(AudioUnit {
                instance: instance,
                maybe_callback: None
            })
        }
    }

    /// Sets the value for some property of the **AudioUnit**.
    ///
    /// To clear an audio unit property value, set the data paramater with `None::<()>`.
    ///
    /// Clearing properties only works for those properties that do not have a default value.
    ///
    /// For more on "properties" see [the reference](https://developer.apple.com/library/ios/documentation/AudioUnit/Reference/AudioUnitPropertiesReference/index.html#//apple_ref/doc/uid/TP40007288).
    ///
    /// **Available** in iOS 2.0 and later.
    ///
    /// Parameters
    /// ----------
    ///
    /// - **id**: The identifier of the property.
    /// - **scope**: The audio unit scope for the property.
    /// - **elem**: The audio unit element for the property.
    /// - **maybe_data**: The value that you want to apply to the property.
    pub fn set_property<T>(&mut self, id: u32, scope: Scope, elem: Element, maybe_data: Option<&T>)
        -> Result<(), Error>
    {
        let (data_ptr, size) = maybe_data.map(|data| {
            let ptr = data as *const _ as *const libc::c_void;
            let size = ::std::mem::size_of::<T>() as u32;
            (ptr, size)
        }).unwrap_or_else(|| (::std::ptr::null(), 0));
        let scope = scope as libc::c_uint;
        let elem = elem as libc::c_uint;
        unsafe {
            try_os_status!(au::AudioUnitSetProperty(self.instance, id, scope, elem, data_ptr, size))
        }
        Ok(())
    }

    /// Gets the value of an **AudioUnit** property.
    ///
    /// **Available** in iOS 2.0 and later.
    ///
    /// Parameters
    /// ----------
    ///
    /// - **id**: The identifier of the property.
    /// - **scope**: The audio unit scope for the property.
    /// - **elem**: The audio unit element for the property.
    pub fn get_property<T>(&self, id: u32, scope: Scope, elem: Element) -> Result<T, Error> {
        let scope = scope as libc::c_uint;
        let elem = elem as libc::c_uint;
        let mut size = ::std::mem::size_of::<T>() as u32;
        unsafe {
            let mut data: T = ::std::mem::uninitialized();
            let data_ptr = &mut data as *mut _ as *mut libc::c_void;
            let size_ptr = &mut size as *mut _;
            try_os_status!(
                au::AudioUnitGetProperty(self.instance, id, scope, elem, data_ptr, size_ptr)
            );
            Ok(data)
        }
    }

    /// Starts an I/O **AudioUnit**, which in turn starts the audio unit processing graph that it is
    /// connected to.
    ///
    /// **Available** in OS X v10.0 and later.
    pub fn start(&mut self) -> Result<(), Error> {
        unsafe { try_os_status!(au::AudioOutputUnitStart(self.instance)); }
        Ok(())
    }

    /// Stops an I/O **AudioUnit**, which in turn stops the audio unit processing graph that it is
    /// connected to.
    ///
    /// **Available** in OS X v10.0 and later.
    pub fn stop(&mut self) -> Result<(), Error> {
        unsafe { try_os_status!(au::AudioOutputUnitStop(self.instance)); }
        Ok(())
    }

    /// Set the **AudioUnit**'s sample rate.
    ///
    /// **Available** in iOS 2.0 and later.
    pub fn set_sample_rate(&mut self, sample_rate: f64) -> Result<(), Error> {
        let id = au::kAudioUnitProperty_SampleRate;
        self.set_property(id, Scope::Input, Element::Output, Some(&sample_rate))
    }

    /// Get the **AudioUnit**'s sample rate.
    pub fn sample_rate(&self) -> Result<f64, Error> {
        let id = au::kAudioUnitProperty_SampleRate;
        self.get_property(id, Scope::Input, Element::Output)
    }

    /// Sets the current **StreamFormat** for the AudioUnit.
    ///
    /// The default format stream is non-interleaved, 32-bit floating point, native-endian, linear
    /// PCM data.
    pub fn set_stream_format(&mut self, stream_format: StreamFormat) -> Result<(), Error> {
        let id = au::kAudioUnitProperty_StreamFormat;
        let asbd = stream_format.to_asbd();
        self.set_property(id, Scope::Input, Element::Output, Some(&asbd))
    }

    /// Return the current Stream Format for the AudioUnit.
    pub fn stream_format(&self) -> Result<StreamFormat, Error> {
        let id = au::kAudioUnitProperty_StreamFormat;
        let asbd = try!(self.get_property(id, Scope::Output, Element::Output));
        StreamFormat::from_asbd(asbd)
    }

}


impl Drop for AudioUnit {
    fn drop(&mut self) {
        unsafe {
            use error;
            use std::error::Error;

            // We don't want to panic in `drop`, so we'll ignore returned errors.
            //
            // A user should explicitly terminate the `AudioUnit` if they want to handle errors (we
            // still need to provide a way to actually do that).
            self.stop().ok();
            error::Error::from_os_status(au::AudioUnitUninitialize(self.instance)).ok();

            self.free_render_callback();
        }
    }
}
