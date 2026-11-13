//! JvDump の出力を SQLite に取り込む（前回の続きから。何度実行しても同じ結果になる）。
//!
//!   cargo run --release --example jv_ingest -- <keiba.db> <jvdata/out> [読んだ位置の名前の接頭辞（例: 2011/）]
use netkeiba_predictor_lib::jvstore;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let db = std::path::PathBuf::from(args.first().expect("DB のパスを指定してください"));
    let out = std::path::PathBuf::from(args.get(1).expect("JvDump の出力先を指定してください"));
    let mut conn = jvstore::open(&db).expect("DB を開けません");
    let t = std::time::Instant::now();
    let prefix = args.get(2).cloned().unwrap_or_default();
    for r in jvstore::ingest_dir_as(&mut conn, &out, &prefix).expect("取り込みに失敗しました") {
        println!(
            "{:<14} {:>12} → {:>12} バイト  反映 {:>8}  削除 {:>5}  対象外 {:>6}",
            r.file, r.from, r.to, r.upserted, r.deleted, r.skipped
        );
    }
    for (name, sql) in [
        ("レース", "SELECT count(*) FROM races"),
        ("うち確定成績", "SELECT count(*) FROM races WHERE kubun = '7'"),
        ("出走", "SELECT count(*) FROM runners"),
        ("調教", "SELECT count(*) FROM works"),
    ] {
        let n: i64 = conn.query_row(sql, [], |r| r.get(0)).unwrap();
        println!("{:<10} {:>10}", name, n);
    }
    println!("{:.1} 秒", t.elapsed().as_secs_f64());
}
