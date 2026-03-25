use std::{
    sync::{mpsc, Arc, Mutex},
    thread,
};

use crate::{image_pipeline::frame_handler::FrameHandler, App};

use ndarray::{prelude::*, OwnedRepr, Shape, ViewRepr};
use slint::{ComponentHandle, Rgb8Pixel, SharedPixelBuffer, Weak};

pub(crate) struct Noise {
    ui: Arc<App>,
    fh: Arc<FrameHandler>,
}

#[derive(Default, PartialEq, Clone, Copy)]
struct NoiseConfig {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    n_frames: usize,
}

type NoiseWindow = Array3<u8>;
type NoiseWindowStack = Array4<u8>;

impl From<NoiseConfig> for NoiseWindowStack {
    fn from(value: NoiseConfig) -> Self {
        Self::zeros((value.w, value.h, 3, value.n_frames))
    }
}

impl Noise {
    pub(crate) fn new(ui: Arc<App>, fh: Arc<FrameHandler>) -> Result<(), ()> {
        let noise = Self { ui, fh };
        noise.start_calculation();
        Ok(())
    }

    fn start_calculation(&self) {
        let new_frame_rx = self.fh.frame_rx.clone();
        let ui_weak = self.ui.as_weak();
        let mut curr_noise_cfg = NoiseConfig::default();
        let mut win_stack = NoiseWindowStack::from(curr_noise_cfg);
        let mut iter_idx = 0;

        let mut calculation_loop = move || loop {
            let pixbuf = new_frame_rx
                .lock()
                .expect("Locking new frame signal failed.")
                .recv()
                .expect("Unable to receive new frame signal.");

            let noise_cfg = Self::get_window_dims(&ui_weak).expect("Did not get noise window dimensions.");
            // reset noise window stack if noise config changed
            if noise_cfg != curr_noise_cfg {
                curr_noise_cfg = noise_cfg;
                win_stack = NoiseWindowStack::from(curr_noise_cfg);
                iter_idx = 0;
            }

            // write window into window stack
            win_stack
                .slice_mut(s![.., .., .., iter_idx])
                .assign(&Self::slice_out_window(pixbuf, &noise_cfg).expect("Could not retrieve noise window."));
            iter_idx = (iter_idx + 1) % curr_noise_cfg.n_frames;

            if win_stack.shape().iter().all(|&e| e > 0) {
                println!(
                    "Sum of red pixels: {}",
                    win_stack.slice(s![0, 0, 0, ..]).mapv(|v| v as u64).sum()
                );
            }
        };

        thread::spawn(move || calculation_loop());
    }

    fn get_window_dims(ui: &Weak<App>) -> Result<NoiseConfig, ()> {
        let (tx, rx) = mpsc::channel::<NoiseConfig>();
        ui.upgrade_in_event_loop(move |ui| {
            tx.send(NoiseConfig {
                x: ui.get_noise_x() as usize,
                y: ui.get_noise_y() as usize,
                w: ui.get_noise_w() as usize,
                h: ui.get_noise_h() as usize,
                n_frames: ui.get_number_of_frames() as usize,
            })
            .expect("Could not send noise window size.");
        })
        .expect("Could not upgrade GUI.");

        Ok(rx.recv().expect("Could not receive noise window size."))
    }

    // queries noise window dimensions from GUI and slices out the window from the provided pixbuf
    fn slice_out_window(pixbuf: SharedPixelBuffer<Rgb8Pixel>, noise_win: &NoiseConfig) -> Result<NoiseWindow, ()> {
        // convert into Array
        let arr = NoiseWindow::from_shape_vec(
            (pixbuf.width() as usize, pixbuf.height() as usize, 3),
            pixbuf.as_bytes().to_vec(),
        )
        .expect("Could not convert pixel buffer to ArrayView.");

        // slice out and return noise window
        let noise_x_end = (noise_win.x + noise_win.w) as usize;
        let noise_y_end = (noise_win.y + noise_win.h) as usize;
        if arr.shape()[0] < noise_x_end || arr.shape()[1] < noise_y_end {
            eprintln!("Noise window too large for input image.");
            return Err(());
        }
        Ok(arr.slice_move(s![noise_win.x..noise_x_end, noise_win.y..noise_y_end, ..]))
    }
}
