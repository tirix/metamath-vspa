use lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, Position, Range as LspRange,
    TextDocumentContentChangeEvent,
};
use metamath_knife::{Database, database::DbOptions};

use crate::proof::ProofWorksheet;

pub(crate) fn mkdb(text: &[u8]) -> Database {
    let options = DbOptions {
        incremental: true,
        ..DbOptions::default()
    };
    let mut db = Database::new(options);
    db.parse(
        "test.mm".to_owned(),
        vec![("test.mm".to_owned(), text.to_owned())],
    );
    db.grammar_pass();
    db
}

fn mkdiag(
    line_from: u32,
    char_from: u32,
    line_to: u32,
    char_to: u32,
    message: &str,
) -> LspDiagnostic {
    LspDiagnostic {
        range: LspRange {
            start: Position {
                line: line_from,
                character: char_from,
            },
            end: Position {
                line: line_to,
                character: char_to,
            },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        message: message.to_owned(),
        ..LspDiagnostic::default()
    }
}

const TEST_DB: &[u8] = b"
    $c |- wff ( ) -> $.
    $( $j syntax 'wff'; syntax '|-' as 'wff'; $)
    $v ph ps ch $.
    wph $f wff ph $.
    wps $f wff ps $.
    wps $f wff ch $.
    wi $a wff ( ph -> ps ) $.
    ${
        min $e |- ph $.
        maj $e |- ( ph -> ps ) $.
        ax-mp $a |- ps $.
    $}
    ax-1 $a |- ( ph -> ( ps -> ph ) ) $.
    ${
        a1i.1 $e |- ph $.
        a1i $p |- ( ps -> ph ) $= ? $.
    $}
";

const TEST_PROOF: &str = "$( <MM> <PROOF_ASST> THEOREM=a1i  LOC_AFTER=?

* Inference introducing an antecedent.  (Contributed by NM, 29-Dec-1992.)

h1::a1i.1      |- ph
2::ax-1        |- ( ph
    -> ( ps -> ph ) )
qed:1,2:ax-mp  |- ( ps -> ph )

$=    ( wi ax-1 ax-mp ) ABADCABEF $.

$)
";

const TEST_PROOF_2: &str = "$( <MM> <PROOF_ASST> THEOREM=a1i  LOC_AFTER=?

* Inference introducing an antecedent.  (Contributed by NM, 29-Dec-1992.)

h1::a1i.1      |- ph
2::ax-1        |- ( ph -> ( ps -> ph ) )
*
* x y
qed:1,2:ax-mp  |- ( ps -> ph )

$=    ( wi ax-1 ax-mp ) ABADCABEF $.

$)
";

#[test]
fn parse_worksheet() {
    let db = &mkdb(TEST_DB);
    let worksheet = ProofWorksheet::from_string(TEST_PROOF.to_string(), db).unwrap();
    assert_eq!(worksheet.steps.len(), 3);
    assert_eq!(worksheet.steps[0].line_idx, 4);
    assert_eq!(worksheet.steps[1].line_idx, 5);
    assert_eq!(worksheet.steps[2].line_idx, 7);
    assert_eq!(worksheet.steps[0].byte_idx, 122);
    assert_eq!(worksheet.steps[1].byte_idx, 143);
    assert_eq!(worksheet.steps[2].byte_idx, 188);
    assert!(worksheet.step_at_line(0).is_none());
    assert_eq!(
        worksheet.byte_to_lsp_position(188),
        Position {
            line: 7,
            character: 0
        }
    );
    assert_eq!(
        worksheet.byte_to_lsp_position(200),
        Position {
            line: 7,
            character: 12
        }
    );
    assert_eq!(
        worksheet.lsp_position_to_byte(Position {
            line: 7,
            character: 0
        }),
        188
    );
    assert_eq!(
        worksheet.lsp_position_to_byte(Position {
            line: 7,
            character: 12
        }),
        200
    );
    assert_eq!(
        worksheet.steps[1].source,
        "2::ax-1        |- ( ph\n    -> ( ps -> ph ) )\n"
    );
    assert_eq!(worksheet.line(7), "qed:1,2:ax-mp  |- ( ps -> ph )");
    assert_eq!(worksheet.line(6), "    -> ( ps -> ph ) )");
    println!("{:#?}", worksheet.diagnostics());
    assert_eq!(worksheet.diagnostics(), vec![]);
}

#[test]
fn parse_worksheet_with_comments() {
    let db = &mkdb(TEST_DB);
    let worksheet = ProofWorksheet::from_string(TEST_PROOF_2.to_string(), db).unwrap();
    assert_eq!(worksheet.steps.len(), 3);
    assert_eq!(worksheet.steps[0].line_idx, 4);
    assert_eq!(worksheet.steps[1].line_idx, 5);
    assert_eq!(worksheet.steps[2].line_idx, 8);
    assert_eq!(worksheet.steps[0].byte_idx, 122);
    assert_eq!(worksheet.steps[1].byte_idx, 143);
    assert_eq!(worksheet.steps[2].byte_idx, 192);
    assert_eq!(
        worksheet.steps[1].source,
        "2::ax-1        |- ( ph -> ( ps -> ph ) )\n*\n* x y\n"
    );
    assert_eq!(
        worksheet.steps[2].source,
        "qed:1,2:ax-mp  |- ( ps -> ph )\n\n$=    ( wi ax-1 ax-mp ) ABADCABEF $.\n\n$)\n"
    );
    assert_eq!(worksheet.line(6), "*");
    assert_eq!(worksheet.line(7), "* x y");
    println!("{:#?}", worksheet.diagnostics());
    assert_eq!(worksheet.diagnostics(), vec![]);
}

#[test]
fn parse_worksheet_diags() {
    let db = &mkdb(TEST_DB);
    let worksheet = ProofWorksheet::from_string(
        "$( <MM> <PROOF_ASST> THEOREM=mp2  LOC_AFTER=?

* A double modus ponens inference.  (Contributed by NM, 5-Apr-1994.)

h1::mp2.1      |- ph
h2::mp2.2      |- ps
h3::mp2.3      |- ( ph -> ( ps -> ch ) )
4:1,3:ax-mp    |- ( ps -> ch )
5x::
6
qed:2,4:ax-mp  |- ch

$=    ( wi ax-mp ) BCEABCGDFHH $.

$)
"
        .to_string(),
        db,
    )
    .unwrap();
    let diags = worksheet.diagnostics();
    println!("{:#?}", diags);
    assert_eq!(diags[0], mkdiag(4, 4, 4, 9, "Unknown theorem"));
    assert_eq!(diags[1], mkdiag(5, 4, 5, 9, "Unknown theorem"));
    assert_eq!(diags[2], mkdiag(6, 4, 6, 9, "Unknown theorem"));
    assert_eq!(diags[3], mkdiag(8, 0, 9, 0, "Could not parse proof line"));
    assert_eq!(diags[4], mkdiag(9, 0, 10, 0, "Could not parse proof line"));
}

#[test]
fn worksheet_insert_new_step() {
    let db = &mkdb(TEST_DB);
    let mut worksheet = ProofWorksheet::from_string(TEST_PROOF.to_string(), db).unwrap();
    worksheet.apply_change(&TextDocumentContentChangeEvent {
        range: Some(LspRange {
            start: Position {
                line: 7,
                character: 0,
            },
            end: Position {
                line: 7,
                character: 0,
            },
        }),
        range_length: None,
        text: "3::ax-1 |- ( ch -> ( ps -> ch ) )\n".to_owned(),
    });
    println!("{:#?}", worksheet.steps);
    assert_eq!(worksheet.steps.len(), 4);
    assert_eq!(worksheet.steps[1].line_idx, 5);
    assert_eq!(worksheet.steps[2].line_idx, 7);
    assert_eq!(worksheet.steps[3].line_idx, 8);
    assert_eq!(worksheet.steps[1].byte_idx, 143);
    assert_eq!(worksheet.steps[2].byte_idx, 188);
    assert_eq!(worksheet.steps[3].byte_idx, 222);
    assert_eq!(
        worksheet.steps[2].source,
        "3::ax-1 |- ( ch -> ( ps -> ch ) )\n"
    );
    println!("{:#?}", worksheet.diagnostics());
    assert_eq!(worksheet.diagnostics(), vec![]);
}

#[test]
fn worksheet_insert_middle() {
    let db = &mkdb(TEST_DB);
    let mut worksheet = ProofWorksheet::from_string(TEST_PROOF.to_string(), db).unwrap();
    worksheet.apply_change(&TextDocumentContentChangeEvent {
        range: Some(LspRange {
            start: Position {
                line: 5,
                character: 20,
            },
            end: Position {
                line: 5,
                character: 22,
            },
        }),
        range_length: None,
        text: "( ps -> ch )".to_owned(),
    });
    println!("{:#?}", worksheet.steps);
    assert_eq!(worksheet.steps.len(), 3);
    assert_eq!(worksheet.steps[0].line_idx, 4);
    assert_eq!(worksheet.steps[1].line_idx, 5);
    assert_eq!(worksheet.steps[2].line_idx, 7);
    assert_eq!(worksheet.steps[0].byte_idx, 122);
    assert_eq!(worksheet.steps[1].byte_idx, 143);
    assert_eq!(worksheet.steps[2].byte_idx, 198);
    assert_eq!(
        worksheet.steps[1].source,
        "2::ax-1        |- ( ( ps -> ch )\n    -> ( ps -> ph ) )\n"
    );
    println!("{:#?}", worksheet.diagnostics());
    assert_eq!(worksheet.diagnostics(), vec![]);
}

#[test]
fn worksheet_insert_newline_before_blank() {
    let db = &mkdb(TEST_DB);
    let mut worksheet = ProofWorksheet::from_string(TEST_PROOF.to_string(), db).unwrap();
    worksheet.apply_change(&TextDocumentContentChangeEvent {
        range: Some(LspRange {
            start: Position {
                line: 6,
                character: 8,
            },
            end: Position {
                line: 6,
                character: 8,
            },
        }),
        range_length: None,
        text: "\n".to_owned(),
    });
    println!("{:#?}", worksheet.steps);
    assert_eq!(worksheet.steps.len(), 3);
    assert_eq!(worksheet.steps[0].line_idx, 4);
    assert_eq!(worksheet.steps[1].line_idx, 5);
    assert_eq!(worksheet.steps[2].line_idx, 8);
    assert_eq!(worksheet.steps[0].byte_idx, 122);
    assert_eq!(worksheet.steps[1].byte_idx, 143);
    assert_eq!(worksheet.steps[2].byte_idx, 189);
    assert_eq!(
        worksheet.steps[1].source,
        "2::ax-1        |- ( ph\n    -> (\n ps -> ph ) )\n"
    );
    println!("{:#?}", worksheet.diagnostics());
    assert_eq!(worksheet.diagnostics(), vec![]);
}

#[test]
fn worksheet_insert_newline_before_char() {
    let db = &mkdb(TEST_DB);
    let mut worksheet = ProofWorksheet::from_string(TEST_PROOF.to_string(), db).unwrap();
    worksheet.apply_change(&TextDocumentContentChangeEvent {
        range: Some(LspRange {
            start: Position {
                line: 6,
                character: 9,
            },
            end: Position {
                line: 6,
                character: 9,
            },
        }),
        range_length: None,
        text: "\n".to_owned(),
    });
    println!("{:#?}", worksheet.steps);
    assert_eq!(worksheet.steps.len(), 4);
    assert_eq!(worksheet.steps[0].line_idx, 4);
    assert_eq!(worksheet.steps[1].line_idx, 5);
    assert_eq!(worksheet.steps[2].line_idx, 7);
    assert_eq!(worksheet.steps[3].line_idx, 8);
    assert_eq!(worksheet.steps[0].byte_idx, 122);
    assert_eq!(worksheet.steps[1].byte_idx, 143);
    assert_eq!(worksheet.steps[2].byte_idx, 176);
    assert_eq!(worksheet.steps[3].byte_idx, 189);
    assert_eq!(
        worksheet.steps[1].source,
        "2::ax-1        |- ( ph\n    -> ( \n"
    );
    assert_eq!(worksheet.steps[2].source, "ps -> ph ) )\n");
    let diags = worksheet.diagnostics();
    println!("{:#?}", diags);
    assert_eq!(diags.len(), 2);
    assert_eq!(diags[0], mkdiag(6, 8, 6, 9, "Parsed statement too short"));
    assert_eq!(diags[1], mkdiag(7, 0, 8, 0, "Could not parse proof line"));
}

#[test]
fn worksheet_insert_newline_at_step_start() {
    let db = &mkdb(TEST_DB);
    let mut worksheet = ProofWorksheet::from_string(TEST_PROOF.to_string(), db).unwrap();
    worksheet.apply_change(&TextDocumentContentChangeEvent {
        range: Some(LspRange {
            start: Position {
                line: 5,
                character: 0,
            },
            end: Position {
                line: 5,
                character: 0,
            },
        }),
        range_length: None,
        text: "\n".to_owned(),
    });
    println!("{:#?}", worksheet.steps);
    assert_eq!(worksheet.steps.len(), 3);
    assert_eq!(worksheet.steps[0].line_idx, 4);
    assert_eq!(worksheet.steps[1].line_idx, 6);
    assert_eq!(worksheet.steps[2].line_idx, 8);
    assert_eq!(worksheet.steps[0].byte_idx, 122);
    assert_eq!(worksheet.steps[1].byte_idx, 144);
    assert_eq!(worksheet.steps[2].byte_idx, 189);
    assert_eq!(worksheet.steps[0].source, "h1::a1i.1      |- ph\n\n");
    assert_eq!(
        worksheet.steps[1].source,
        "2::ax-1        |- ( ph\n    -> ( ps -> ph ) )\n"
    );
    assert_eq!(worksheet.diagnostics(), vec![]);
}
