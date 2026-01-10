//! Contains functionality to query v4l2 video source device capabilities for raw video.

use std::fmt::Display;

use gst::prelude::*;

#[derive(Debug, Clone)]
pub struct RawSourceCaps {
    pub curr_cap_idx: usize,
    pub curr_framerate_idx: usize,
    caps: Vec<RawSourceCap>,
}

#[derive(Debug, Clone)]
pub struct RawSourceCap {
    pub width: i32,
    pub height: i32,
    pub framerates: gst::List,
    pub device_path: String,
}

impl Display for RawSourceCap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "video/x-raw, Resolution: {}x{}, Framerates: {:?}, Device Path: {}",
            self.width, self.height, self.framerates, self.device_path,
        )
    }
}

impl RawSourceCap {
    pub fn to_string_wo_framerate(&self) -> String {
        format!("{}: {}x{}", self.device_path, self.width, self.height)
    }
}

/// Contains the v4l2 video source device capabilities for raw video available on the system.
impl RawSourceCaps {
    /// Creates a new RawSourceCaps by querying the system for available v4l2 video source devices.
    pub fn new() -> Self {
        RawSourceCaps {
            caps: Self::get_raw_source_caps(),
            curr_cap_idx: 0,
            curr_framerate_idx: 0,
        }
    }

    /// Updates the RawSourceCaps by re-querying the system for available v4l2 video source devices.
    pub fn update(&mut self) {
        self.caps = Self::get_raw_source_caps();
    }

    /// Returns the available raw source capabilities.
    pub fn get_caps(&self) -> &Vec<RawSourceCap> {
        &self.caps
    }

    /// Returns the number of available raw source capabilities.
    pub fn len(&self) -> usize {
        self.caps.len()
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

            // print supported caps
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
                        let device_cap = RawSourceCap {
                            width,
                            height,
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
