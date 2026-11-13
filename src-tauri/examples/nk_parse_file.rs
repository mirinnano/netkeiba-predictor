fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let html = std::fs::read_to_string(&a[0]).unwrap();
    let r = netkeiba_predictor_lib::scraper::parse_result_as_jvrace(&html, &a[1], "20231029").unwrap();
    println!("surface={} turf={:?} dirt={:?} first3f={:?} last3f={:?} track={} cond={}", r.surface, r.turf_cond, r.dirt_cond, r.first3f, r.last3f, r.track_code, r.condition_min);
}
