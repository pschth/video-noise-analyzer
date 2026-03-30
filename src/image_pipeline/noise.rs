use std::{
    sync::{mpsc, Arc},
    thread,
};

use crate::{image_pipeline::frame_handler::FrameHandler, App};

use ndarray::prelude::*;
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

#[derive(Debug)]
struct FixedPatternNoise {
    fpn: f32,
    row: f32,
    col: f32,
}

type NoiseWindow = Array2<f32>;
type NoiseWindowStack = Array3<f32>;

impl From<NoiseConfig> for NoiseWindowStack {
    fn from(value: NoiseConfig) -> Self {
        Self::zeros((value.h, value.w, value.n_frames))
    }
}

impl NoiseConfig {
    fn is_valid(&self) -> bool {
        self.w > 0 && self.h > 0 && self.n_frames > 0
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
            if !noise_cfg.is_valid() {
                continue;
            }

            // reset noise window stack if noise config changed
            if noise_cfg != curr_noise_cfg {
                curr_noise_cfg = noise_cfg;
                win_stack = NoiseWindowStack::from(curr_noise_cfg);
                iter_idx = 0;
            }

            // write window into window stack
            win_stack
                .slice_mut(s![.., .., iter_idx])
                .assign(&Self::slice_out_window(pixbuf, &noise_cfg).expect("Could not retrieve noise window."));
            iter_idx = (iter_idx + 1) % curr_noise_cfg.n_frames;

            // calculate noise metrics for each full window stack
            if iter_idx == 0 {
                Self::update_noise_metrics(win_stack.clone(), ui_weak.clone());
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
        let arr = Array3::<u8>::from_shape_vec(
            (pixbuf.height() as usize, pixbuf.width() as usize, 3 as usize),
            pixbuf.as_bytes().to_vec(),
        )
        .expect("Could not convert pixel buffer to ArrayView.");
        let arr = arr.mapv(|v| v as f32);

        // slice out and return noise window and convert to luminance
        let noise_x_end = (noise_win.x + noise_win.w) as usize;
        let noise_y_end = (noise_win.y + noise_win.h) as usize;
        if arr.shape()[0] < noise_y_end || arr.shape()[1] < noise_x_end {
            eprintln!("Noise window too large for input image.");
            return Err(());
        }

        let red = arr.slice(s![noise_win.y..noise_y_end, noise_win.x..noise_x_end, 0]);
        let green = arr.slice(s![noise_win.y..noise_y_end, noise_win.x..noise_x_end, 1]);
        let blue = arr.slice(s![noise_win.y..noise_y_end, noise_win.x..noise_x_end, 2]);

        // Y conversion according to BT.601
        Ok(0.299 * &red + 0.587 * &green + 0.114 * &blue)
    }

    // calculates noise metrics and updates GUI values in a separate thread
    fn update_noise_metrics(win_stack: NoiseWindowStack, ui: Weak<App>) {
        thread::spawn(move || {
            let fpn = Self::calc_fixed_pattern_noise(&win_stack);
            let tn = Self::calc_temporal_noise(&win_stack);
            ui.upgrade_in_event_loop(move |ui| {
                ui.set_fixed_pattern_noise(fpn.fpn);
                ui.set_temporal_noise(tn);
            })
            .expect("UI could not be upgraded.");
        });
    }

    fn calc_fixed_pattern_noise(win_stack: &NoiseWindowStack) -> FixedPatternNoise {
        let temporal_mean = win_stack.mean_axis(Axis(2)).expect("Temporal mean calculation failed.");

        let fpn = temporal_mean.std(0.0);
        let row = win_stack
            .mean_axis(Axis(1))
            .expect("Calculating column mean failed.")
            .std(0.0);
        let col = win_stack
            .mean_axis(Axis(0))
            .expect("Calculating row mean failed.")
            .std(0.0);

        FixedPatternNoise { fpn, row, col }
    }

    fn calc_temporal_noise(win_stack: &NoiseWindowStack) -> f32 {
        let temporal_std = win_stack.std_axis(Axis(2), 0.0);
        temporal_std
            .mean()
            .expect("Mean of temporal std could not be calculated.")
    }
}
