#![allow(unused)]

use std::io::{Read, Write};
use std::process::exit;
use tracing_subscriber::EnvFilter;

use crate::msg::{AudioDeviceT, AudioHostT};

pub fn init_logger() {
    let _ = tracing_log::LogTracer::init();
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .try_init();
}

#[macro_export]
macro_rules! syslog {
    ($system_out:expr,$message:expr) => {{
        use blaulicht_shared::LogLevel;
        use $crate::msg::SystemMessage;

        // The UI receiver may already be gone during shutdown; don't panic.
        let _ = $system_out.send(SystemMessage::Log(($message).into(), LogLevel::Info));
    }};

    ($system_out:expr,$message:expr,$level:expr) => {{
        use $crate::msg::SystemMessage;

        let _ = $system_out.send(SystemMessage::Log(($message).into(), $level));
    }};
}

#[cfg(feature = "audio")]
mod with_audio_feature {
    use crate::msg::{AudioDeviceT, AudioHostT};
    use cpal::{
        traits::{DeviceTrait, HostTrait},
        Device, HostId,
    };

    /// Returns all valid and available input devices.
    fn get_input_devices() -> Vec<(cpal::HostId, Vec<cpal::Device>)> {
        cpal::available_hosts()
            .into_iter()
            .map(|host_id| {
                let host = cpal::host_from_id(host_id).expect("should know the just queried host");
                (host_id, host)
            })
            .map(|(host_id, host)| (host_id, host.devices()))
            .filter(|(_, devices)| devices.is_ok())
            .map(|(host_id, devices)| (host_id, devices.unwrap()))
            .map(|(host_id, devices)| {
                (
                    host_id,
                    devices
                        .into_iter()
                        // check: is input device?
                        .filter(|dev| dev.default_input_config().is_ok())
                        // check: can we get its name?
                        .filter(|dev| dev.name().is_ok())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
    }

    pub fn get_input_devices_flat() -> Vec<(cpal::HostId, cpal::Device)> {
        get_input_devices()
            .into_iter()
            .flat_map(|(host_id, devices)| {
                devices
                    .into_iter()
                    .map(|dev| (host_id, dev))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    }

    pub fn device_from_name(dev_id: String) -> Option<AudioDeviceT> {
        let devices = get_input_devices_flat();

        let selected_device: Option<(HostId, Device)> =
            devices.into_iter().find(|(this_host, this_dev)| {
                let this_id = this_dev.name().unwrap();
                this_id == dev_id
            });

        let Some((_, device)) = selected_device else {
            return None;
        };

        Some(device)
    }

    pub fn device_from_names(host_id: String, dev_id: String) -> Option<AudioDeviceT> {
        let devices = get_input_devices_flat();

        let selected_device: Option<(HostId, Device)> =
            devices.into_iter().find(|(this_host, this_dev)| {
                this_host.name() == host_id && this_dev.name().unwrap() == dev_id
            });

        let Some((_, device)) = selected_device else {
            return None;
        };

        Some(device)
    }
}

pub fn device_from_name(dev_id: String) -> Option<AudioDeviceT> {
    #[cfg(feature = "audio")]
    return with_audio_feature::device_from_name(dev_id);

    #[cfg(not(feature = "audio"))]
    return Some(AudioDeviceT::default());
}

pub fn device_from_names(host_id: String, dev_id: String) -> Option<AudioDeviceT> {
    #[cfg(feature = "audio")]
    return with_audio_feature::device_from_names(host_id, dev_id);

    #[cfg(not(feature = "audio"))]
    return Some(AudioDeviceT::default());
}

pub fn get_input_devices_flat() -> Vec<(AudioHostT, AudioDeviceT)> {
    #[cfg(feature = "audio")]
    return with_audio_feature::get_input_devices_flat();

    #[cfg(not(feature = "audio"))]
    return vec![(AudioHostT::default(), AudioDeviceT::default())];
}
