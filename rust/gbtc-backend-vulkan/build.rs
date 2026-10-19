use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn glslc_path() -> String {
    env::var("GLSLC").unwrap_or_else(|_| "glslc".to_string())
}

fn spirv_val_path() -> Option<String> {
    env::var("SPIRV_VAL")
        .ok()
        .or_else(|| Some("spirv-val".to_string()))
}

fn compile_shader(glslc: &str, input: &Path, output: &Path) -> Result<(), String> {
    let status = Command::new(glslc)
        .arg("-fshader-stage=compute")
        .arg("--target-env=vulkan1.1")
        .arg(input)
        .arg("-o")
        .arg(output)
        .status();
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!(
            "glslc failed with status {} compiling {}",
            s,
            input.display()
        )),
        Err(e) => Err(format!("failed to invoke glslc ({glslc}): {e}")),
    }
}

fn validate_spirv(spirv_val: &str, spv: &Path) {
    match Command::new(spirv_val)
        .arg("--target-env")
        .arg("vulkan1.1")
        .arg(spv)
        .status()
    {
        Ok(s) if s.success() => {}
        Ok(s) => panic!(
            "spirv-val failed with status {} validating {}",
            s,
            spv.display()
        ),
        Err(e) => panic!("failed to invoke spirv-val ({spirv_val}): {e}"),
    }
}

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let manifest_dir = PathBuf::from(manifest_dir);
    let shader_dir = manifest_dir.join("../shaders");
    let generator = shader_dir.join("generate.py");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));

    println!(
        "cargo:rerun-if-changed={}",
        shader_dir.join("sha256d.comp").display()
    );
    println!("cargo:rerun-if-changed={}", generator.display());
    println!("cargo:rerun-if-env-changed=GLSLC");
    println!("cargo:rerun-if-env-changed=SPIRV_VAL");
    // Generate variant sources into OUT_DIR.
    let gen_status = Command::new("python3")
        .arg(&generator)
        .arg("--out-dir")
        .arg(&out_dir)
        .status();
    match gen_status {
        Ok(s) if s.success() => {}
        Ok(s) => panic!("generate.py failed with status {}", s),
        Err(e) => panic!("failed to invoke generate.py: {e}"),
    }

    let glslc = glslc_path();

    // Preserve looped reference.
    let reference_input = shader_dir.join("sha256d.comp");
    let reference_output = out_dir.join("sha256d.spv");
    if let Err(e) = compile_shader(&glslc, &reference_input, &reference_output) {
        panic!("{}", e);
    }

    let variants = ["unrolled", "pre3", "pruned", "boolean"];
    for variant in variants {
        let input = out_dir.join(format!("sha256d-generated-{}.comp", variant));
        let output = out_dir.join(format!("{}.spv", variant));
        if let Err(e) = compile_shader(&glslc, &input, &output) {
            panic!("{}", e);
        }
    }

    if let Some(spirv_val) = spirv_val_path() {
        validate_spirv(&spirv_val, &reference_output);
        for variant in variants {
            validate_spirv(&spirv_val, &out_dir.join(format!("{}.spv", variant)));
        }
    }
}
