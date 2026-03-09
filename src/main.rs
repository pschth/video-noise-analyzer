mod image_pipeline;

use image_pipeline::ImagePipeline;

use std::error::Error;

slint::slint! {
    export { App } from "ui/app-window.slint";
}

fn main() -> Result<(), Box<dyn Error>> {
    let ui = App::new()?;

    let mut img_pipe = ImagePipeline::new().expect("Failed to create image pipeline");
    ui.link_with_image_pipeline(&mut img_pipe)
        .expect("Failed to link image pipeline to GUI.");

    ui.run()?;

    let _ = img_pipe.set_state(gst::State::Null);

    Ok(())
}
