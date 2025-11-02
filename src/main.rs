mod image_pipeline;

use image_pipeline::gstreamer::ImagePipeline;
use std::error::Error;

slint::slint! {
    export { App } from "ui/app-window.slint";
}

fn main() -> Result<(), Box<dyn Error>> {
    let ui = App::new()?;

    let pipeline = ImagePipeline::new(ui.as_weak()).expect("Failed to create image pipeline");

    // let caps = pipeline.get_device_capabilities();
    // for (idx, cap) in caps.iter().enumerate() {
    //     println!("Device cap {idx}: {cap}");
    // }

    ui.run()?;

    let _ = pipeline.set_state(gst::State::Null);

    Ok(())
}
