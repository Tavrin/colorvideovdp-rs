use std::time::Instant;
fn main() {
    let count = std::env::args()
        .nth(1)
        .map(|n| n.parse::<usize>().expect("case count"))
        .unwrap_or(100_000);
    let mut state = 0x4356_5644_5046_555au64;
    for (name, target) in [
        (
            "image_predict",
            colorvideovdp_fuzz::image_predict as fn(&[u8]) -> bool,
        ),
        (
            "video_predict",
            colorvideovdp_fuzz::video_predict as fn(&[u8]) -> bool,
        ),
        (
            "display_parse",
            colorvideovdp_fuzz::display_parse as fn(&[u8]) -> bool,
        ),
    ] {
        let start = Instant::now();
        let mut accepted = 0;
        for case in 0..count {
            let mut data = vec![0u8; 64 + case % 193];
            for byte in &mut data {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *byte = state as u8;
            }
            // Retain a valid route as well as unconstrained bit-pattern inputs.
            if case % 64 == 0 {
                data[0] = 5;
                data[1] = 4;
                data[2] = 4;
                data[3] = 1;
                data[4] = 0;
                data[5] = 2;
                data[6] = 5;
            }
            if name == "display_parse" && case % 64 == 0 {
                data = br#"{"fuzz":{"resolution":[1920,1080],"pixels_per_degree":45,"max_luminance":200,"contrast":1000}}"#.to_vec();
            }
            // Panics and aborts terminate the run and preserve a reproducible seed/index.
            accepted += target(&data) as usize;
        }
        println!("{name}: {count} cases, {accepted} accepted, {} rejected, {:.3} seconds, seed 0x435656445046555a, no crashes", count - accepted, start.elapsed().as_secs_f64());
    }
}
