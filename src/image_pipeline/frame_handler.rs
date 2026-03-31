use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver},
        Arc, Mutex,
    },
};

use chrono::prelude::*;
use gst::{glib::object::Cast, prelude::GstBinExt};
use image::RgbImage;
use slint::{Image, Rgb8Pixel, SharedPixelBuffer, Weak};

use crate::App;

#[derive(Clone)]
pub struct FrameHandler {
    cat: Image,
    ui: Arc<Weak<App>>,
    pub frame_rx: Arc<Mutex<Receiver<SharedPixelBuffer<Rgb8Pixel>>>>,
}

// handles the display of frames in the GUI window
impl FrameHandler {
    pub fn init(pipeline: &gst::Pipeline, ui: Weak<App>) -> Self {
        // set up link of image pipeline output frames to GUI
        let new_frame_callback: fn(App, Image) = |ui, new_frame| {
            if ui.get_playing() {
                ui.set_video_frame(new_frame);
            }
        };

        let ui = Arc::new(ui);
        let rx = Self::register_frame_callback(pipeline, ui.clone(), new_frame_callback)
            .expect("Failed to register new frame callback");

        // load pause image (cat)
        let cat_path = Path::new("ui/assets/cat.jpg");
        let Ok(cat) = Image::load_from_path(cat_path) else {
            panic!("No cat found. Terrible!");
        };

        FrameHandler {
            cat,
            ui,
            frame_rx: Arc::new(Mutex::new(rx)),
        }
    }

    pub fn display_pause_image(&self) {
        println!("Catting the window!");
        self.ui
            .upgrade()
            .expect("Could not upgrade UI.")
            .set_video_frame(self.cat.clone());
    }

    pub fn take_screenshot(&self) {
        let ui = self.ui.upgrade().expect("Could not upgrade UI.");
        if !ui.get_playing() {
            println!("No Video playing.");
            return;
        }

        let output_dir = PathBuf::from(ui.get_output_dir().as_str());
        if !output_dir.exists() {
            if let Err(e) = fs::create_dir_all(&output_dir) {
                eprintln!("Could not create screenshot output directory: {e}");
                return;
            };
            println!("Created screenshot output directory {output_dir:?}.")
        }

        let Some(rgb_image) = get_frame_as_rgbimage(&ui) else {
            return;
        };

        let mut file_name = Local::now().format("%Y-%m-%d_%H:%M:%S%.3f").to_string();
        file_name.push_str("_screenshot.png");
        let output_name = output_dir.join(&file_name);

        if let Err(e) = rgb_image.save(&output_name) {
            eprintln!("Saving screenshot failed: {e}.");
            return;
        }
        println!("Screenshot successfully saved to {output_name:?}.");
    }

    fn register_frame_callback<AppHandle: slint::ComponentHandle + 'static>(
        pipeline: &gst::Pipeline,
        ui: Arc<Weak<AppHandle>>,
        new_frame_cb: fn(AppHandle, Image),
    ) -> Result<Receiver<SharedPixelBuffer<Rgb8Pixel>>, ()> {
        let sink = pipeline
            .by_name("appsink0")
            .expect("Could not find appsink element in pipeline.")
            .downcast::<gst_app::AppSink>()
            .expect("Could not downcast Element to AppSink.");

        let (tx, rx) = mpsc::channel::<SharedPixelBuffer<Rgb8Pixel>>();
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |appsink| {
                    let Ok(sample) = appsink.pull_sample() else {
                        println!("Failed to pull sample from appsink.");
                        return Err(gst::FlowError::Eos);
                    };

                    let Some(buffer) = sample.buffer() else {
                        println!("Sample has no buffer.");
                        return Err(gst::FlowError::Error);
                    };
                    let Ok(map) = buffer.map_readable() else {
                        println!("Failed to map buffer readable.");
                        return Err(gst::FlowError::Error);
                    };
                    let data = map.as_slice();

                    // Create an Image from the raw RGB data
                    let video_info = gst_video::VideoInfo::from_caps(sample.caps().unwrap()).unwrap();
                    let pixel_buffer =
                        SharedPixelBuffer::<Rgb8Pixel>::clone_from_slice(data, video_info.width(), video_info.height());

                    let buf_clone = pixel_buffer.clone();
                    ui.upgrade_in_event_loop(move |ui| {
                        new_frame_cb(ui, Image::from_rgb8(buf_clone));
                    })
                    .expect("Could not upgrade UI in event loop.");

                    // send signal that new frame is ready
                    tx.send(pixel_buffer).expect("Sending new frame signal failed.");

                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        Ok(rx)
    }
}

#[inline]
fn get_frame_as_rgbimage(ui: &App) -> Option<RgbImage> {
    let frame = ui.get_video_frame();
    let Some(frame_buf) = frame.to_rgb8() else {
        eprintln!("Could not obtain pixel buffer from video frame.");
        return None;
    };

    let frame_size = frame.size();
    let Some(rgb_image) = RgbImage::from_raw(frame_size.width, frame_size.height, frame_buf.as_bytes().to_vec()) else {
        eprintln!("Video frame not convertable to RGB image.");
        return None;
    };

    Some(rgb_image)
}
