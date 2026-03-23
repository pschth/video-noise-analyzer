use std::sync::Arc;

use gst::{Pipeline, State};
use rfd::FileDialog;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use thiserror::Error;

use crate::image_pipeline::noise::Noise;
use crate::image_pipeline::{frame_handler::FrameHandler, gstreamer::ImagePipeline};

use crate::App;

#[derive(Debug, Error)]
pub enum UiError {
    #[error("UI already initialized.")]
    AlreadyInitialized,
}

impl App {
    // set up link of image pipeline output frames to GUI
    pub fn link_with_image_pipeline(&self, image_pipeline: &mut ImagePipeline) -> Result<(), UiError> {
        if image_pipeline.frame_handler.is_some() {
            return Err(UiError::AlreadyInitialized);
        }

        image_pipeline
            .frame_handler
            .replace(FrameHandler::init(&image_pipeline.pipeline, self.as_weak()));

        self.init_gui_callbacks(image_pipeline);
        self.init_gui_elements(image_pipeline);

        Ok(())
    }

    fn init_gui_callbacks(&self, image_pipeline: &mut ImagePipeline) {
        let ui_weak = Arc::new(self.as_weak());

        // set up link of image pipeline video controls to GUI
        let pipe = Arc::new(image_pipeline.pipeline.clone());
        let fh = Arc::new(image_pipeline.frame_handler.clone().unwrap());
        let ui = Arc::new(ui_weak.upgrade().expect("Could not upgrade UI."));

        // set up callbacks for video controls
        ui.init_on_toggle_play_pause(pipe.clone(), fh.clone());
        ui.init_on_take_screenshot(fh.clone());
        ui.init_on_selected_video_source(pipe.clone(), &image_pipeline);
        ui.init_on_selected_framerate(pipe.clone(), &image_pipeline);
        ui.init_on_choose_output_dir();
        ui.start_noise_calculation(fh.clone());
    }

    fn init_on_toggle_play_pause(self: &Arc<App>, pipe: Arc<Pipeline>, fh: Arc<FrameHandler>) {
        self.on_toggle_play_pause({
            let ui = self.clone();
            move || match ImagePipeline::toggle_play_pause(&pipe, &fh) {
                State::Playing => ui.set_playing(true),
                _ => ui.set_playing(false),
            }
        });
    }

    fn init_on_take_screenshot(self: &Arc<App>, fh: Arc<FrameHandler>) {
        self.on_take_screenshot(move || fh.take_screenshot());
    }

    fn init_on_selected_video_source(self: &Arc<App>, pipe: Arc<Pipeline>, img: &ImagePipeline) {
        self.on_selected_video_source({
            let ui = self.clone();
            let pipeline_arc = pipe.clone();
            let caps_arc = img.caps.clone();
            move |value| {
                let selected_idx = ui.get_current_video_source() as usize;
                println!("Selected video source: {value}, index: {selected_idx}");

                if ImagePipeline::set_video_resolution(pipeline_arc.as_ref(), selected_idx, caps_arc.clone()).is_err() {
                    eprintln!("Failed to set video properties for selected source.");
                };
                // reset framerate to first index when changing the resolution
                ui.set_curr_fps(0);
            }
        });
    }

    fn init_on_selected_framerate(self: &Arc<App>, pipe: Arc<Pipeline>, img: &ImagePipeline) {
        self.on_selected_framerate({
            let ui = self.clone();
            let pipeline_arc = pipe.clone();
            let caps_arc = img.caps.clone();
            move |value| {
                let selected_idx = ui.get_curr_fps() as usize;
                println!("Selected framerate: {value}, index: {selected_idx}");

                if ImagePipeline::set_framerate(pipeline_arc.as_ref(), selected_idx, caps_arc.clone()).is_err() {
                    eprintln!("Failed to set video properties for selected source.");
                };
            }
        });
    }

    fn init_on_choose_output_dir(self: &Arc<App>) {
        self.on_choose_output_dir({
            let ui = self.clone();
            move || {
                let dir = FileDialog::new().set_directory("/").pick_folder();
                if dir.is_some() {
                    ui.set_output_dir(dir.unwrap().to_string_lossy().to_string().into());
                }
            }
        });
    }

    fn init_gui_elements(&self, image_pipeline: &mut ImagePipeline) {
        let cap_lock = image_pipeline.caps.lock().expect("Could not acquire lock");
        let cap_vec = cap_lock.get_caps();
        let available_sources: VecModel<SharedString> =
            cap_vec.iter().map(|s| s.to_string_wo_framerate().into()).collect();
        let ui = Arc::new(self.as_weak().upgrade().expect("Could not upgrade UI."));
        ui.set_video_sources(ModelRc::new(available_sources));

        let available_framerates: VecModel<SharedString> = cap_lock
            .get_current_framerates_as_strings()
            .iter()
            .map(|s| s.into())
            .collect();
        ui.set_framerates(ModelRc::new(available_framerates));
    }

    fn start_noise_calculation(self: Arc<App>, fh: Arc<FrameHandler>) {
        Noise::new(self, fh).expect("Could not start noise calculation.");
    }
}
