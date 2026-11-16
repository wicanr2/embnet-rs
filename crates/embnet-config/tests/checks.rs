//! 每項檢查各有一個會失敗的設定，從 good.toml 改一處而來。
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use embnet_config::{check, parse, type_hash};

const GOOD: &str = include_str!("fixtures/good.toml");

fn codes(text: &str) -> Vec<&'static str> {
    let cfg = parse(text).expect("TOML 解析失敗");
    let mut v: Vec<&'static str> = check(&cfg).violations.iter().map(|x| x.check).collect();
    v.dedup();
    v
}

fn replace_once(text: &str, from: &str, to: &str) -> String {
    assert!(text.contains(from), "找不到要替換的片段：{from}");
    text.replacen(from, to, 1)
}

#[test]
fn good_passes() {
    let cfg = parse(GOOD).unwrap();
    let rep = check(&cfg);
    assert!(rep.passed(), "{:#?}", rep.violations);
    assert_eq!(rep.checks, 10);
    // speed_cmd：host 經 eth0、Bridge 轉進 can1，兩段都有上限
    let f: Vec<_> = rep.flows.iter().filter(|f| f.subject == "speed_cmd").collect();
    assert_eq!(f.len(), 2);
    let can = f.iter().find(|f| f.link == "can1").unwrap();
    let eth = f.iter().find(|f| f.link == "eth0").unwrap();
    assert!(can.path_bound_us > eth.path_bound_us);
    assert!((can.path_bound_us - (eth.path_bound_us + can.bound_us)).abs() < 1e-6);
    println!(
        "EMBNET_CONFIG_TEST good max_util={:.2}% worst_bound_us={:.0}",
        rep.max_util() * 100.0,
        rep.worst_bound_us()
    );
}

#[test]
fn c1_duplicate_node_id() {
    assert_eq!(codes(&replace_once(GOOD, "id = 20", "id = 10")), ["C1"]);
}

#[test]
fn c1_reserved_subject_id() {
    assert_eq!(codes(&replace_once(GOOD, "id = 300", "id = 8190")), ["C1"]);
}

#[test]
fn c2_missing_type() {
    assert_eq!(codes(&replace_once(GOOD, "type = \"demo.Imu\"", "type = \"demo.Nope\"")), ["C2"]);
}

#[test]
fn c3_protected_without_deadline() {
    assert_eq!(codes(&replace_once(GOOD, "deadline_us = 50_000\n", "")), ["C3"]);
}

#[test]
fn c3_mac_without_key() {
    assert_eq!(codes(&replace_once(GOOD, "protection = \"e2e32\"", "protection = \"mac32\"")), ["C3"]);
}

#[test]
fn c4_tid_timeout_too_long() {
    assert_eq!(codes(&replace_once(GOOD, "tid_timeout_us = 100_000\n\n[[subject]]\nname = \"motor_state\"",
        "tid_timeout_us = 400_000\n\n[[subject]]\nname = \"motor_state\"")), ["C4"]);
}

#[test]
fn c5_bridge_not_on_link() {
    let t = replace_once(GOOD, "links = [\"eth0\", \"can1\", \"uart0\"]", "links = [\"eth0\", \"can1\"]");
    assert!(codes(&t).contains(&"C5"));
}

#[test]
fn c6_second_path_into_same_link() {
    // 第二台 Bridge 也把 speed_cmd 從 eth0 送進 can1：同一條 CAN 上出現兩個發送者（K-27）
    let extra = "\n[[node]]\nname = \"bridge2\"\nid = 2\nlinks = [\"eth0\", \"can1\"]\nbridge = true\nlatency_us = 200\n\n\
                 [[route]]\nsubject = \"speed_cmd\"\nbridge = \"bridge2\"\nfrom = \"eth0\"\nto = \"can1\"\n";
    assert_eq!(codes(&format!("{GOOD}{extra}")), ["C6"]);
}

#[test]
fn c6_route_back_into_publisher_link() {
    let extra = "\n[[route]]\nsubject = \"speed_cmd\"\nbridge = \"bridge1\"\nfrom = \"can1\"\nto = \"eth0\"\n";
    assert_eq!(codes(&format!("{GOOD}{extra}")), ["C6"]);
}

#[test]
fn c7_overloaded_link() {
    assert!(codes(&replace_once(GOOD, "period_us = 20_000", "period_us = 100")).contains(&"C7"));
}

#[test]
fn c8_deadline_too_tight() {
    assert_eq!(codes(&replace_once(GOOD, "deadline_us = 50_000", "deadline_us = 300")), ["C8"]);
}

#[test]
fn c9_uart_payload_too_small() {
    assert_eq!(codes(&replace_once(GOOD, "max_payload = 64", "max_payload = 16")), ["C9"]);
}

#[test]
fn unknown_field_is_rejected() {
    assert!(parse(&replace_once(GOOD, "zone = \"machine\"", "zone = \"machine\"\nzonee = 1")).is_err());
}

#[test]
fn type_hash_matches_vectors() {
    let v: serde_json_lite::Value = serde_json_lite::parse(include_str!("../../../docs/spec/vectors/type-hash.json"));
    for c in v.cases {
        let t = embnet_config::Type { name: c.name, major: c.major, max_len: c.max_len, fields: c.fields };
        assert_eq!(format!("{:016X}", type_hash(&t)), c.type_hash);
    }
}

/// 只解 type-hash.json 用到的欄位，避免為測試多拉一個 JSON 相依。
mod serde_json_lite {
    pub struct Case {
        pub name: String,
        pub major: u32,
        pub max_len: usize,
        pub fields: String,
        pub type_hash: String,
    }
    pub struct Value {
        pub cases: Vec<Case>,
    }
    fn field<'a>(obj: &'a str, key: &str) -> &'a str {
        let k = format!("\"{key}\": ");
        let start = obj.find(&k).unwrap() + k.len();
        let rest = &obj[start..];
        if let Some(r) = rest.strip_prefix('"') {
            &r[..r.find('"').unwrap()]
        } else {
            let end = rest.find([',', '\n', '}']).unwrap();
            rest[..end].trim()
        }
    }
    pub fn parse(text: &str) -> Value {
        let body = &text[text.find("\"cases\"").unwrap()..];
        let cases = body
            .split('{')
            .skip(1)
            .map(|obj| Case {
                name: field(obj, "name").to_string(),
                major: field(obj, "major").parse().unwrap(),
                max_len: field(obj, "max_len").parse().unwrap(),
                fields: field(obj, "fields").to_string(),
                type_hash: field(obj, "type_hash").to_string(),
            })
            .collect();
        Value { cases }
    }
}

#[test]
fn c10_mac_verification_over_budget() {
    // speed_cmd 改用 mac32 並加速到每 50 µs 一筆：跨 zone（external 到 machine），Bridge 每秒要驗 20,000 則
    let t = replace_once(GOOD, "protection = \"e2e32\"", "protection = \"mac32\"\nkey = 1");
    let ok = codes(&t);
    assert!(!ok.contains(&"C10"), "每 10 ms 一筆應該在預算內：{ok:?}");
    let t = replace_once(&t, "period_us = 10_000\njitter_us = 1_000", "period_us = 50\njitter_us = 0");
    assert!(codes(&t).contains(&"C10"));
}
