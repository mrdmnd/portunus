use std::{
    io::Cursor,
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
    _start: Instant,
}

impl GraphicsCaptureApiHandler for Capture {
    type Flags = String;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(_message: Self::Flags) -> Result<Self, Self::Error> {
        Ok(Self {
            _start: Instant::now(),
        })
    }

    // Called every time a new frame is available.
    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        const DATA_FRAME_WIDTH: u32 = 33;
        const DATA_FRAME_HEIGHT: u32 = 331;

        let mut cropped = frame
            .buffer_crop(0, 0, DATA_FRAME_WIDTH, DATA_FRAME_HEIGHT)
            .map_err(|e| format!("Failed to crop frame: {}", e))?;
        let frame_data_buffer = cropped
            .as_raw_nopadding_buffer()
            .map_err(|e| format!("Failed to get raw buffer: {}", e))?;

        // Read the transmission metadata from the first pixel.
        let compression = frame_data_buffer[0] / 255;
        let num_bytes = frame_data_buffer[1] as usize + 255 * frame_data_buffer[2] as usize;
        println!("Compression on: {}", compression);
        println!("Number of bytes transmitted by addon: {}", num_bytes);

        let alpha_filtered_bytes: Vec<u8> = frame_data_buffer[4..]
            .chunks_exact(4)
            .take(num_bytes / 3)
            .flat_map(|chunk| chunk.iter().take(3))
            .cloned()
            .collect();
        println!("Byte stream sent by addon: {:?}", alpha_filtered_bytes);

        let decompressed_byte_stream = match compression {
            0 => alpha_filtered_bytes,
            _ => DeflateDecoder::new(&alpha_filtered_bytes.as_slice())
                .decode_zlib()
                .map_err(|e| format!("Failed to decompress: {}", e))?,
        };
        println!("Decompressed bytes: {:?}", decompressed_byte_stream);

        let mut deserializer = Deserializer::new(Cursor::new(decompressed_byte_stream));
        let deserialized_data: Value = serde_json::Value::deserialize(&mut deserializer)
            .map_err(|e| format!("Failed to deserialized data: {}", e))?;

        println!("Deserialized data: {}", deserialized_data);

        std::thread::sleep(time::Duration::from_millis(200));
        Ok(())
    }

    // Optional handler called when the capture item (usually a window) closes.
    fn on_closed(&mut self) -> Result<(), Self::Error> {
        println!("Capture Session Closed");
        Ok(())
    }
}

pub fn start_capture() {
    let primary_monitor = Monitor::primary().expect("There is no primary monitor?");
    let settings = Settings::new(
        primary_monitor,
        CursorCaptureSettings::WithoutCursor,
        DrawBorderSettings::WithoutBorder,
        ColorFormat::Rgba8,
        "".to_string(),
    );
    //Capture::start(settings).expect("Screen capture failed.");
    Capture::start_free_threaded(settings).expect("Screen capture thread failed to launch.");
}
