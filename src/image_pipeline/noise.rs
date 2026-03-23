use std::{
    sync::{Arc, Mutex},
    thread,
};

use crate::{image_pipeline::frame_handler::FrameHandler, App};

use ndarray::{prelude::*, Shape, ViewRepr};
use slint::{Rgb8Pixel, SharedPixelBuffer};

pub(crate) struct Noise {
    ui: Arc<App>,
    fh: Arc<FrameHandler>,
    number_of_frames: u32,
}

impl Noise {
    pub(crate) fn new(ui: Arc<App>, fh: Arc<FrameHandler>) -> Result<(), ()> {
        let number_of_frames = ui.get_number_of_frames() as u32;
        let noise = Self {
            ui,
            fh,
            number_of_frames,
        };
        noise.start_calculation();
        Ok(())
    }

    fn start_calculation(&self) {
        let new_frame_rx = self.fh.frame_rx.clone();
        let ui = self.ui.clone();
        let calculation_loop = move || loop {
            let pixbuf = new_frame_rx
                .lock()
                .expect("Locking new frame signal failed.")
                .recv()
                .expect("Unable to receive new frame signal.");
            Self::get_window(pixbuf);
        };

        thread::spawn(move || calculation_loop());
    }

    fn get_window(pixbuf: SharedPixelBuffer<Rgb8Pixel>) {
        let arrview = ArrayView::from_shape(
            (pixbuf.width() as usize, pixbuf.height() as usize, 3),
            pixbuf.as_bytes(),
        )
        .expect("Could not convert pixel buffer to ArrayView.");

        println!("shape: {:?}", arrview.shape());

        // let window = arrview.slice(s![noise_x..noise_w, noise_y..noise_h, ..]);
        // ui.set_temporal_noise(window.len() as f32);
    }
}
