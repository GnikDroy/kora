use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use inkwell::module::Module;
use inkwell::targets::FileType;

use super::LinkErr;
use super::optimize::{optimize, target_machine};

const RUNTIME_LIB: &[u8] = include_bytes!(env!("KORA_RUNTIME"));

pub fn link(llvm: &Module, output: &Path, opt: &str, link_args: &[String]) -> Result<(), LinkErr> {
    llvm.verify()
        .unwrap_or_else(|e| panic!("invalid IR generated:\n{e}"));

    let msvc = cfg!(target_env = "msvc");

    let object_path = output.with_extension(if msvc { "obj" } else { "o" });
    emit_object_file(llvm, &object_path, opt)?;
    let runtime_path = runtime_sibling(output, if msvc { "lib" } else { "a" });

    let status = std::fs::write(&runtime_path, RUNTIME_LIB)
        .map_err(LinkErr::Io)
        .and_then(|_| {
            if msvc {
                link_msvc(&object_path, &runtime_path, output, link_args)
            } else {
                link_gnu(&object_path, &runtime_path, output, link_args)
            }
        });
    std::fs::remove_file(&object_path).ok();
    std::fs::remove_file(&runtime_path).ok();
    if !status?.success() {
        return Err(LinkErr::LinkFailed);
    }
    Ok(())
}

fn runtime_sibling(output: &Path, ext: &str) -> PathBuf {
    let stem = output
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("kora");
    output.with_file_name(format!("{stem}_runtime.{ext}"))
}

fn link_gnu(
    object: &Path,
    runtime: &Path,
    output: &Path,
    link_args: &[String],
) -> Result<ExitStatus, LinkErr> {
    let dead_strip = if cfg!(target_os = "macos") {
        "-Wl,-dead_strip"
    } else {
        "-Wl,--gc-sections"
    };
    let mut cmd = Command::new("cc");
    cmd.arg(object)
        .arg(runtime)
        .arg("-lm")
        .arg("-pthread")
        .args(link_args);
    if cfg!(target_os = "windows") {
        cmd.arg("-lws2_32");
    }
    cmd.arg(dead_strip).arg("-o").arg(output);
    cmd.status().map_err(LinkErr::Io)
}

fn link_msvc(
    object: &Path,
    runtime: &Path,
    output: &Path,
    link_args: &[String],
) -> Result<ExitStatus, LinkErr> {
    let mut cmd = Command::new("clang-cl");
    cmd.arg("/nologo")
        .arg(object)
        .arg(runtime)
        .arg("ws2_32.lib")
        .arg("user32.lib")
        .args(link_args)
        .arg(format!("/Fe:{}", output.display()))
        .arg("/link")
        .arg("/OPT:REF")
        .arg("/SUBSYSTEM:CONSOLE")
        .arg("/DEFAULTLIB:msvcrt.lib")
        .arg("/DEFAULTLIB:ucrt.lib")
        .arg("/DEFAULTLIB:vcruntime.lib");
    cmd.status().map_err(LinkErr::Io)
}

fn emit_object_file(llvm: &Module, object: &Path, opt: &str) -> Result<(), LinkErr> {
    let machine = target_machine(opt)?;
    optimize(llvm, &machine, opt)?;
    machine
        .write_to_file(llvm, FileType::Object, object)
        .map_err(|e| LinkErr::EmitObject(e.to_string()))
}
