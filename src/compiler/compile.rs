//! The compile process: a project manifests a program, and the program
//! becomes an executable.
//!
//! The stages run in order: `build` loads the manifest's tree, resolves every
//! name into an edge, and checks every body; then the root `main` is found,
//! what it never reaches is pruned, the result is lowered to bytecode, and the
//! image is packaged at the manifest's output. A failed build reports
//! everything it could still earn and writes no output.

use std::path::Path;

use super::warnings::{is_program_source, unused_declarations};
use crate::compiler::diagnostic::render;
use crate::compiler::{
    Build, DIAGNOSTIC_PREFIX, MISSING_MAIN, ProgramSource, attach_trailer, build, lower_bytecode,
    mark_reachable, program_main, prune_unreachable, serialize_bytecode,
};
use crate::project::Project;
use package::make_writable_executable;

pub(crate) mod build;
pub(crate) mod check;
pub(crate) mod load;
pub(crate) mod lower;
pub(crate) mod package;
pub(crate) mod prune;
pub(crate) mod resolve;

#[cfg(test)]
mod tests;

/// Compile the project to its output by copying `executable` (the running
/// `kc`) and attaching the program. On failure, returns the message to print,
/// already rendered.
pub(crate) fn compile(
    project: &Project,
    executable: &Path,
    source: &impl ProgramSource,
) -> Result<(), String> {
    let Build {
        loaded,
        mut program,
        warnings,
    } = build(&project.root, source, &project.warnings)
        .map_err(|failure| failure.rendered().to_owned())?;
    let mut facts: Vec<_> = warnings
        .into_iter()
        .filter(|fact| is_program_source(&fact.path) && admitted(project, fact))
        .collect();
    // The entry is the manifest's program main. A missing one is the
    // project's fact; a malformed one is the file's, and the diagnostic names
    // it.
    let mut missing_main = false;
    let mut reachable = None;
    let main = match program_main(&program) {
        Ok(Some(main)) => {
            // The reachable set is the compile's second reading of the graph:
            // what no reachable body reads is a warning, then it is dropped
            // before lowering.
            let marked = mark_reachable(&program, main);
            facts.extend(
                unused_declarations(&program, &marked)
                    .into_iter()
                    .filter(|fact| admitted(project, fact)),
            );
            reachable = Some(marked);
            Some(main)
        }
        Ok(None) => {
            missing_main = true;
            None
        }
        Err(diagnostic) => {
            facts.push(diagnostic);
            None
        }
    };
    // A recovered tree cannot become a binary, but it still reports: every
    // syntax error it kept, every fact the recovered source earned, and the
    // entry's own fact.
    if !loaded.syntax.is_empty() || main.is_none() {
        let mut message = String::new();
        for diagnostic in loaded.syntax.iter().chain(facts.iter()) {
            message.push_str(&render(&loaded, diagnostic));
        }
        if missing_main {
            message.push_str(&format!(
                "{DIAGNOSTIC_PREFIX}: {}: {MISSING_MAIN}\n",
                project.manifest.display()
            ));
        }
        return Err(message);
    }
    for warning in &facts {
        eprint!("{}", render(&loaded, warning));
    }
    let main = main.expect("the entry was settled above");
    let reachable = reachable.expect("a setted entry has its reachable set");
    let main = prune_unreachable(&mut program, main, &reachable);
    let bytecode = lower_bytecode(&program, main);

    let bytes = serialize_bytecode(&bytecode)
        .map_err(|error| format!("{DIAGNOSTIC_PREFIX}: cannot serialize the program: {error}\n"))?;
    let output_path = &project.output;
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "{DIAGNOSTIC_PREFIX}: cannot create {}: {error}\n",
                parent.display()
            )
        })?;
    }
    std::fs::copy(executable, output_path).map_err(|error| {
        format!(
            "{DIAGNOSTIC_PREFIX}: cannot write {}: {error}\n",
            output_path.display()
        )
    })?;
    // A copy keeps the source's mode, and an installed `kc` may be read-only
    // (a package manager or a Nix store path), so make the output a writable
    // executable before the trailer is appended to it.
    make_writable_executable(output_path).map_err(|error| {
        format!(
            "{DIAGNOSTIC_PREFIX}: cannot write {}: {error}\n",
            output_path.display()
        )
    })?;
    attach_trailer(output_path, &bytes).map_err(|error| {
        format!(
            "{DIAGNOSTIC_PREFIX}: cannot write {}: {error}\n",
            output_path.display()
        )
    })?;
    Ok(())
}

/// Whether the project's diagnostics policy admits one fact.
fn admitted(project: &Project, fact: &crate::compiler::Diagnostic) -> bool {
    project.warnings.admits_fact(fact)
}
