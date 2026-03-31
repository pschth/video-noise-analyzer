# Video Noise Analyzer

Just a little example project to play around with gstreamer, Slint and ndarray.

The **Video Noise Analyzer** calculates the video noise (temporal noise and fixed pattern noise) of V4L2 devices.

You can select the source to be analyzed out of all available V4L2 devices. Select the noise window either directly in the video content or using the text boxes. The video noise is analyzed automatically for the specified number of frames.

## How to run

Just build and run using `cargo run`.
