use std::{
    io::{self, Cursor, Write},
    process::exit,
    time::{self, Instant},
};

use serde::Deserialize;
use windows_capture::{
    capture::GraphicsCaptureApiHandler,
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
    monitor::Monitor,
    settings::{ColorFormat, CursorCaptureSettings, DrawBorderSettings, Settings},
};

use rmp_serde::Deserializer;
use serde_json::Value;
use zune_inflate::DeflateDecoder;

struct Capture {
    start: Instant,
}

impl GraphicsCaptureApiHandler for Capture {
    // The type of flags used to get the values from the settings.
    type Flags = String;

    // The type of error that can occur during capture, the error will be returned from `CaptureControl` and `start` functions.
    type Error = Box<dyn std::error::Error + Send + Sync>;

    // Function that will be called to create the struct. The flags can be passed from settings.
    fn new(_message: Self::Flags) -> Result<Self, Self::Error> {
        Ok(Self {
            start: Instant::now(),
        })
    }

    // Called every time a new frame is available.
    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let data_frame_width = 33;
        let data_frame_height = 331;

        let mut cropped = frame
            .buffer_crop(0, 0, data_frame_width, data_frame_height)
            .unwrap();
        let frame_data_buffer = cropped.as_raw_nopadding_buffer().unwrap();
        //println!("FDB: {:?}", frame_data_buffer);
        //capture_control.stop();

        // Read the transmission metadata from the first pixel.
        let compression = &frame_data_buffer[0] / 255;
        let num_bytes: usize = (&frame_data_buffer[1] + 255 * &frame_data_buffer[2]).into();
        println!("Compression on: {}", compression);
        println!("Real bytes sent by addon: {}", num_bytes);

        // Read the buffer body data
        // For we actually need to look at ()
        let real_data = &frame_data_buffer[4..];
        let mut new_data: Vec<u8> = Vec::new();
        for i in 0..num_bytes / 3 {
            new_data.push(real_data[4 * i + 0]);
            new_data.push(real_data[4 * i + 1]);
            new_data.push(real_data[4 * i + 2]);
        }
        println!("Byte stream sent by addon: {:?}", new_data);

        let decompressed_byte_stream = if compression > 0 {
            DeflateDecoder::new(new_data.as_mut_slice())
                .decode_zlib()
                .unwrap()
        } else {
            new_data.clone()
        };
        println!("Decompressed bytes: {:?}", decompressed_byte_stream);

        let mut deserializer = Deserializer::new(Cursor::new(decompressed_byte_stream));
        let deserialized_data: Value = serde_json::Value::deserialize(&mut deserializer).unwrap();
        println!("Deserialized data: {}", deserialized_data);

        if self.start.elapsed().as_secs() >= 6 {
            capture_control.stop();
            println!();
        }
        std::thread::sleep(time::Duration::from_millis(1000));
        Ok(())
    }

    // Optional handler called when the capture item (usually a window) closes.
    fn on_closed(&mut self) -> Result<(), Self::Error> {
        println!("Capture Session Closed");
        Ok(())
    }
}

fn main() {
    let primary_monitor = Monitor::primary().expect("There is no primary monitor?");
    let settings = Settings::new(
        primary_monitor,
        CursorCaptureSettings::WithoutCursor,
        DrawBorderSettings::WithoutBorder,
        ColorFormat::Rgba8,
        "".to_string(),
    );

    // Starts the capture and takes control of the current thread
    Capture::start(settings).expect("Screen capture failed.");
}
