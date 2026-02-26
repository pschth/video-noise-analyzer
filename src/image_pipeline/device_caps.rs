//! Contains functionality to query v4l2 video source device capabilities for raw video.

use std::fmt::Display;

use gst::prelude::*;

use crate::image_pipeline::gstreamer::GstError;

#[derive(Debug, Clone)]
pub struct RawSourceCaps {
    curr_cap_idx: usize,
    curr_framerate_idx: usize,
    caps: Vec<RawSourceCap>,
}

#[derive(Debug, Clone)]
pub struct RawSourceCap {
    pub resolution: Resolution,
    framerates: gst::List,
    device_path: String,
}

#[derive(Debug, Clone)]
pub struct Resolution {
    pub width: i32,
    pub height: i32,
}

impl Display for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

impl Display for RawSourceCap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "video/x-raw, Resolution: {}, Framerates: {:?}, Device Path: {}",
            self.resolution, self.framerates, self.device_path,
        )
    }
}

impl RawSourceCap {
    pub fn to_string_wo_framerate(&self) -> String {
        format!("{}: {}", self.device_path, self.resolution)
    }
}

/// Contains the v4l2 video source device capabilities for raw video available on the system.
impl RawSourceCaps {
    /// Creates a new RawSourceCaps by querying the system for available v4l2 video source devices.
    pub fn new() -> Result<Self, GstError> {
        let rsc = RawSourceCaps {
            caps: Self::get_raw_source_caps(),
            curr_cap_idx: 0,
            curr_framerate_idx: 0,
        };

        if rsc.caps.is_empty() {
            eprintln!("No video capabilities found.");
            Err(GstError::NoCapsFound)
        } else {
            Ok(rsc)
        }
    }

    /// Returns the available raw source capabilities.
    pub fn get_caps(&self) -> &Vec<RawSourceCap> {
        &self.caps
    }

    pub fn set_resolution(&mut self, cap_idx: usize) -> Result<(), GstError> {
        if cap_idx >= self.caps.len() {
            eprintln!("Invalid resolution index.");
            return Err(GstError::InvalidValue);
        }
        self.curr_cap_idx = cap_idx;
        // reset frame rate to index 0 since it is possible that the new resolution has different available frame rates
        self.curr_framerate_idx = 0;
        Ok(())
    }

    pub fn set_framerate(&mut self, framerate_idx: usize) -> Result<(), GstError> {
        if framerate_idx >= self.caps[self.curr_cap_idx].framerates.len() {
            eprintln!("Invalid frame rate index.");
            return Err(GstError::InvalidValue);
        }
        self.curr_framerate_idx = framerate_idx;
        Ok(())
    }

    pub fn get_current_device_path(&self) -> &str {
        self.caps.get(self.curr_cap_idx).unwrap().device_path.as_str()
    }

    pub fn get_current_framerates_as_strings(&self) -> Vec<String> {
        let curr_cap = self.caps.get(self.curr_cap_idx).unwrap();
        curr_cap
            .framerates
            .iter()
            .map(|f| {
                let frac = f.get::<gst::Fraction>().unwrap();
                String::from(format!("{}/{}", frac.numer(), frac.denom()))
            })
            .collect()
    }

    /// Returns the currently active framerate.
    pub fn get_current_framerate(&self) -> Result<gst::Fraction, GstError> {
        let framerate = self
            .caps
            .get(self.curr_cap_idx)
            .and_then(|cap| cap.framerates.get(self.curr_framerate_idx))
            .ok_or_else(|| {
                eprintln!("Could not get frame rate with index {}", self.curr_framerate_idx);
                GstError::InvalidValue
            })?;

        framerate.get::<gst::Fraction>().map_err(|_| {
            println!("Could not convert framerate from device capabilities.");
            GstError::NoCapsFound
        })
    }

    /// Returns the currently active resolution.
    pub fn get_current_resolution(&self) -> Resolution {
        self.caps.get(self.curr_cap_idx).unwrap().resolution.clone()
    }

    /// Queries all available v4l2 video source devices and their raw capabilities.
    fn get_raw_source_caps() -> Vec<RawSourceCap> {
        let mut device_caps: Vec<RawSourceCap> = vec![];

        // device monitor to list video sources
        let monitor = gst::DeviceMonitor::new();
        monitor.add_filter(Some("Video/Source"), None);
        monitor.start().expect("Could not start device monitor");

        for device in monitor.devices() {
            let display_name = device.display_name();
            // get linux video source device path, e.g. /dev/video0
            let properties = match device.properties() {
                None => {
                    println!("Device {display_name} has no properties.");
                    continue;
                }
                Some(props) => props,
            };
            let device_path = match properties.get::<String>("api.v4l2.path") {
                Err(_) => {
                    println!("Device {display_name} has no api.v4l2.path property.");
                    continue;
                }
                Ok(path) => path,
            };

            if let Some(caps_list) = device.caps() {
                println!("Available v4l2 caps:");
                for caps in caps_list.iter() {
                    if caps.name() != "video/x-raw" {
                        continue;
                    }

                    let width = caps.get::<i32>("width");
                    let height = caps.get::<i32>("height");
                    let framerates = caps.get::<gst::List>("framerate");

                    if let (Ok(width), Ok(height), Ok(framerates)) = (width, height, framerates) {
                        let resolution = Resolution { width, height };
                        let fr: Vec<gst::Fraction> =
                            framerates.iter().map(|f| f.get::<gst::Fraction>().unwrap()).collect();
                        println!("Resolution: {}, framerates: {:?}", &resolution, fr);
                        let device_cap = RawSourceCap {
                            resolution,
                            framerates,
                            device_path: device_path.clone(),
                        };
                        device_caps.push(device_cap);
                    } else {
                        println!("  Could not get width/height/framerate/image_format for caps: {caps:?}");
                    }
                }
            } else {
                println!("Device {display_name} has no caps.");
            }
        }

        device_caps
    }
}
