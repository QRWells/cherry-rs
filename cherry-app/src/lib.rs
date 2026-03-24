pub fn output_filename(backend_id: &str, frame_index: Option<u32>) -> String {
    let sanitized = backend_id.replace('.', "-");
    match frame_index {
        Some(index) => format!("{}-{:04}.png", sanitized, index),
        None => format!("{}.png", sanitized),
    }
}

#[cfg(test)]
mod tests {
    use super::output_filename;

    #[test]
    fn filename_for_single_frame_is_deterministic() {
        assert_eq!(output_filename("ray.normal", None), "ray-normal.png");
    }

    #[test]
    fn filename_for_sequence_is_indexed() {
        assert_eq!(
            output_filename("raster.simple", Some(0)),
            "raster-simple-0000.png"
        );
        assert_eq!(
            output_filename("raster.simple", Some(12)),
            "raster-simple-0012.png"
        );
    }
}
