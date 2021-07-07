use crate::compile::{CompileError, CompileStruct, FileInfo};
use crate::mavm::Value;
use crate::run::{run, Machine, RuntimeEnvironment};
use crate::uint256::Uint256;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

fn compile_run_cycle(input: String) -> Machine {
    let mut compile = CompileStruct::default();
    compile.input = vec![input];
    compile.test_mode = true;
    compile.consts_file = Some(format!("arb_os/constants.json"));
    let mexe = match compile.invoke() {
        Ok((mexe, _error_system)) => mexe,
        Err(_error_system) => panic!("failed to compile"),
    };
    let mut machine = Machine::new(mexe, RuntimeEnvironment::new(None));
    run(&mut machine, vec![], false).unwrap();
    machine
}

#[test]
fn test_basic() {
    let machine = compile_run_cycle("test-programs/basic.mini".to_string());
    assert_eq!(machine.stack_top(), None);
}

#[test]
fn test_xif_else() {
    let machine = compile_run_cycle("test-programs/xif-else.mini".to_string());
    assert_eq!(machine.stack_top(), Some(&Value::Int(Uint256::zero())));
}

#[test]
fn test_codeblocks() {
    let machine = compile_run_cycle("test-programs/codeblocks.mini".to_string());
    assert_eq!(
        machine.stack_top(),
        Some(&Value::Tuple(Arc::new(vec![
            Value::Int(Uint256::zero()),
            Value::Int(Uint256::from_u64(25))
        ])))
    );
}

#[test]
fn test_error_system() {
    fn check_correctness(
        issues: Vec<CompileError>,
        correct: &[&[usize]],
        file_info_chart: &BTreeMap<u64, FileInfo>,
        file_path: &str,
        sources: &BTreeSet<String>,
        issue_type: &str,
    ) {
        let mut issues: Vec<CompileError> = issues
            .into_iter()
            .filter(|x| x.locations.len() > 0)
            .collect();

        issues.sort_by(|a, b| {
            a.locations
                .last()
                .unwrap()
                .line
                .to_usize()
                .cmp(&b.locations.last().unwrap().line.to_usize())
        });

        let issues: Vec<Vec<usize>> = issues
            .into_iter()
            .map(|issue| {
                issue
                    .locations
                    .into_iter()
                    .filter(|loc| {
                        sources.contains(&file_info_chart.get(&loc.file_id).unwrap().name)
                    })
                    .map(|loc| loc.line.to_usize() + 1)
                    .collect::<Vec<usize>>()
            })
            .filter(|vec| !vec.is_empty())
            .collect();

        let pairs = issues.iter().zip(correct.iter());

        for (issue_lines, correct_lines) in pairs {
            let mut found_issue = issue_lines.len() != correct_lines.len();

            for (issue_line, correct_line) in issue_lines.iter().zip(correct_lines.iter()) {
                found_issue = found_issue || issue_line != correct_line;
            }

            if found_issue {
                let last_warning = issue_lines.last().unwrap();
                let last_correct = correct_lines.last().unwrap();

                if last_warning < last_correct {
                    panic!(
                        "Unexpected {} on line {} in file {}",
                        issue_type, last_warning, file_path
                    );
                } else if last_warning > last_correct {
                    panic!(
                        "Missing {} on line {} in file {}",
                        issue_type, last_correct, file_path
                    );
                } else {
                    panic!(
                        "{} mismatch on line {} in file {}",
                        issue_type, last_correct, file_path
                    );
                }
            }
        }

        if issues.len() != correct.len() {
            panic!(
                "Found {} {}s when {} are expected in {}",
                issues.len(),
                issue_type,
                correct.len(),
                file_path
            );
        }
    }

    fn check_issues(
        file_path: &str,
        sources: BTreeSet<String>,
        correct_warnings: &[&[usize]],
        correct_errors: &[&[usize]],
    ) {
        let mut compile = CompileStruct::default();
        compile.input = vec![file_path.to_string()];
        compile.warnings_are_errors = true;
        compile.consts_file = Some("minitests/constants.json".to_string());

        let (warnings, errors, file_info_chart) = match compile.invoke() {
            Ok(_) => panic!("No compile error was emitted despite the -w flag."),
            Err(error_system) => {
                error_system.print();
                (
                    error_system.warnings,
                    error_system.errors,
                    error_system.file_info_chart,
                )
            }
        };

        check_correctness(
            warnings,
            correct_warnings,
            &file_info_chart,
            file_path,
            &sources,
            "warning",
        );
        check_correctness(
            errors,
            correct_errors,
            &file_info_chart,
            file_path,
            &sources,
            "error",
        );
    }

    // check single-file warnings
    check_issues(
        "minitests/error-system-test.mini",
        vec!["error-system-test".to_string()].into_iter().collect(),
        &[
            &[7],
            &[8],
            &[10, 11],
            &[12, 13],
            &[18],
            &[19],
            &[20],
            &[32],
            &[45],
            &[51],
            &[51],
            &[53],
            &[61],
            &[62],
            &[66],
            &[70],
            &[72],
            &[73],
            &[76],
            &[76],
            &[87],
            &[89],
            &[94],
            &[96],
            &[84, 86, 100],
            &[104],
            &[109],
            &[116],
            &[102, 104, 119],
            &[122],
            &[127],
            &[135],
            &[147],
            &[152],
            &[155],
            &[159],
            &[179],
            &[184],
            &[190],
            &[190],
            &[193],
            &[194],
            &[196],
            &[197],
        ],
        &[&[7], &[45], &[105], &[187]],
    );

    // check directory callgraph warnings
    check_issues(
        "minitests/callgraph",
        vec!["main".to_string(), "other".to_string()]
            .into_iter()
            .collect(),
        &[
            &[15],
            &[17], // special warning from other.mini
            &[19],
            &[20],
            &[21],
            &[26],
        ],
        &[],
    );
}
