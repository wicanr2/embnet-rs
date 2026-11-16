//! `embnet-config <設定檔>`：執行建置期檢查，印出通過行（`docs/spec/07-config.md` 第四節）。

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let verbose = std::env::var_os("EMBNET_VERBOSE").is_some();
    let Some(path) = args.next() else {
        eprintln!("用法：embnet-config <設定檔.toml>");
        return ExitCode::from(2);
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("讀不到 {path}：{e}");
            return ExitCode::from(2);
        }
    };
    let cfg = match embnet_config::parse(&text) {
        Ok(c) => c,
        Err(e) => {
            println!("EMBNET_CONFIG_FAIL PARSE {}", e.replace('\n', " "));
            println!("EMBNET_CONFIG FAIL file={path} checks=0");
            return ExitCode::from(1);
        }
    };
    let rep = embnet_config::check(&cfg);
    if verbose {
        for t in &cfg.types {
            println!("type {} hash={:016X}", t.name, embnet_config::type_hash(t));
        }
        for (l, u) in &rep.utilization {
            println!("link {l} util={:.2}%", u * 100.0);
        }
        for f in &rep.flows {
            println!(
                "flow {} on {} prio={} bits={:.0} r={:.0}bit/s b={:.0}bit bound_us={:.1} path_bound_us={:.1}",
                f.subject, f.link, f.priority, f.bits, f.r, f.b, f.bound_us, f.path_bound_us
            );
        }
    }
    for v in &rep.violations {
        println!("EMBNET_CONFIG_FAIL {} {}", v.check, v.message);
    }
    let status = if rep.passed() { "PASS" } else { "FAIL" };
    println!(
        "EMBNET_CONFIG {status} file={path} checks={} links={} subjects={} routes={} max_util={:.1}% worst_bound_us={:.0}",
        rep.checks,
        cfg.links.len(),
        cfg.subjects.len(),
        cfg.routes.len(),
        rep.max_util() * 100.0,
        rep.worst_bound_us()
    );
    if rep.passed() { ExitCode::SUCCESS } else { ExitCode::from(1) }
}
