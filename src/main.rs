// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use gst::{prelude::*, MessageView};
use std::{error::Error, io::pipe, rc::Rc, sync::Arc};

use futures::stream::StreamExt;

use slint::Model;

slint::slint! {
    export { Logic } from "ui/logic.slint";
    export { App } from "ui/app-window.slint";
}

fn main() -> Result<(), Box<dyn Error>> {
    gst::init().unwrap();

    let source = gst::ElementFactory::make("v4l2src")
        .property("device", "/dev/video0")
        .build()
        .expect("Could not create source element.");

    let capsfilter = gst::ElementFactory::make("capsfilter")
        .name("filter")
        .build()
        .expect("Could not create capsfilter element.");

    let caps = gst::Caps::builder("image/jpeg")
        .field("width", 1920)
        .field("height", 1080)
        .field("framerate", gst::Fraction::new(30, 1))
        .build();
    capsfilter.set_property("caps", &caps);

    let decoder = gst::ElementFactory::make("jpegdec")
        .name("decoder")
        .build()
        .expect("Could not create decoder element.");

    let convert = gst::ElementFactory::make("videoconvert")
        .name("convert")
        .build()
        .expect("Could not create convert element.");

    let sink = gst::ElementFactory::make("autovideosink")
        .build()
        .expect("Could not create sink element.");

    let pipeline = gst::Pipeline::new();
    pipeline
        .add_many([&source, &capsfilter, &decoder, &convert, &sink])
        .unwrap();
    gst::Element::link_many([&source, &capsfilter, &decoder, &convert, &sink]).unwrap();

    pipeline.set_state(gst::State::Playing).unwrap();

    let bus = pipeline.bus().unwrap();
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        match msg.view() {
            MessageView::Eos(..) => {
                println!("End of stream");
                break;
            }
            MessageView::Error(err) => {
                eprintln!(
                    "Error from {:?}: {} ({:?})",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                );
                break;
            }
            _ => println!("Other message: {:?}", msg),
        }
    }

    pipeline.set_state(gst::State::Null).unwrap();

    Ok(())
}
