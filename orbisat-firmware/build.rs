use std::{env, fs, path::Path};

fn main() {
    generate_sweep_const();
    linker_be_nice();
    println!("cargo:rustc-link-arg=-Tdefmt.x");
    // make sure linkall.x is the last linker script (otherwise might cause problems with flip-link)
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}

fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let kind = &args[1];
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                "_defmt_timestamp" => {
                    eprintln!();
                    eprintln!(
                        "💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`"
                    );
                    eprintln!();
                }
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                "esp_rtos_initialized" | "esp_rtos_yield_task" | "esp_rtos_task_create" => {
                    eprintln!();
                    eprintln!(
                        "💡 `esp-radio` has no scheduler enabled. Make sure you have initialized `esp-rtos` or provided an external scheduler."
                    );
                    eprintln!();
                }
                "embedded_test_linker_file_not_added_to_rustflags" => {
                    eprintln!();
                    eprintln!(
                        "💡 `embedded-test` not found - make sure `embedded-test.x` is added as a linker script for tests"
                    );
                    eprintln!();
                }
                _ => (),
            },
            // we don't have anything helpful for "missing-lib" yet
            _ => {
                std::process::exit(1);
            }
        }

        std::process::exit(0);
    }

    println!(
        "cargo:rustc-link-arg=-Wl,--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}

fn generate_sweep_const() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("sweep.rs");

    let source = fs::read_to_string("sweep.csv").unwrap();
    let sweep = source
        .lines()
        .map(|x| x.split(','))
        .map(|mut x| (x.next().unwrap(), x.next().unwrap()))
        .map(|(f, d)| format!("({f}, {d})"))
        .collect::<Vec<_>>();
    let count = sweep.len();
    let sweep = sweep.join(",");

    let generated_code = format!("pub const SWEEP: [(u32, u32); {count}] = [{sweep}];");

    fs::write(&dest_path, generated_code).unwrap();

    println!("cargo:rerun-if-changed=sweep.csv");
}
