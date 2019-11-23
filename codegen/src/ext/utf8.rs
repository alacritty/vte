//! Macro expansion for the utf8 parser state table
use std::fmt;

use syntex::Registry;

use syntex_syntax::ast::{self, Arm, Expr, ExprKind, LitKind, Pat, PatKind};
use syntex_syntax::codemap::Span;
use syntex_syntax::ext::base::{DummyResult, ExtCtxt, MacEager, MacResult};
use syntex_syntax::ext::build::AstBuilder;
use syntex_syntax::parse::parser::Parser;
use syntex_syntax::parse::token::{DelimToken, Token};
use syntex_syntax::parse::PResult;
use syntex_syntax::ptr::P;
use syntex_syntax::tokenstream::TokenTree;

#[path = "../../../utf8parse/src/types.rs"]
mod types;

use self::types::{pack, Action, State};

pub fn register(registry: &mut Registry) {
    registry.add_macro("utf8_state_table", expand_state_table);
}

fn state_from_str<S>(s: &S) -> Result<State, ()>
where
    S: AsRef<str>,
{
    Ok(match s.as_ref() {
        "State::Ground" => State::Ground,
        "State::Tail3" => State::Tail3,
        "State::Tail2" => State::Tail2,
        "State::Tail1" => State::Tail1,
        "State::U3_2_e0" => State::U3_2_e0,
        "State::U3_2_ed" => State::U3_2_ed,
        "State::Utf8_4_3_f0" => State::Utf8_4_3_f0,
        "State::Utf8_4_3_f4" => State::Utf8_4_3_f4,
        _ => return Err(()),
    })
}

fn action_from_str<S>(s: &S) -> Result<Action, ()>
where
    S: AsRef<str>,
{
    Ok(match s.as_ref() {
        "Action::InvalidSequence" => Action::InvalidSequence,
        "Action::EmitByte" => Action::EmitByte,
        "Action::SetByte1" => Action::SetByte1,
        "Action::SetByte2" => Action::SetByte2,
        "Action::SetByte2Top" => Action::SetByte2Top,
        "Action::SetByte3" => Action::SetByte3,
        "Action::SetByte3Top" => Action::SetByte3Top,
        "Action::SetByte4" => Action::SetByte4,
        _ => return Err(()),
    })
}

fn parse_table_input_mappings<'a>(parser: &mut Parser<'a>) -> PResult<'a, Vec<Arm>> {
    // Must start on open brace
    parser.expect(&Token::OpenDelim(DelimToken::Brace))?;

    let mut arms: Vec<Arm> = Vec::new();
    while parser.token != Token::CloseDelim(DelimToken::Brace) {
        match parser.parse_arm() {
            Ok(arm) => arms.push(arm),
            Err(e) => {
                // Recover by skipping to the end of the block.
                return Err(e);
            },
        }
    }

    // Consume the closing brace
    parser.bump();
    Ok(arms)
}

/// Expressions describing state transitions and actions
#[derive(Debug)]
struct TableDefinitionExprs {
    state_expr: P<Expr>,
    mapping_arms: Vec<Arm>,
}

fn state_from_expr(expr: P<Expr>, cx: &mut ExtCtxt) -> Result<State, ()> {
    let s = match expr.node {
        ExprKind::Path(ref _qself, ref path) => path.to_string(),
        _ => {
            cx.span_err(expr.span, "expected State");
            return Err(());
        },
    };

    state_from_str(&s).map_err(|_| {
        cx.span_err(expr.span, "expected State");
    })
}

fn u8_lit_from_expr(expr: &Expr, cx: &mut ExtCtxt) -> Result<u8, ()> {
    static MSG: &str = "expected u8 int literal";

    match expr.node {
        ExprKind::Lit(ref lit) => match lit.node {
            LitKind::Int(val, _) => Ok(val as u8),
            _ => {
                cx.span_err(lit.span, MSG);
                Err(())
            },
        },
        _ => {
            cx.span_err(expr.span, MSG);
            Err(())
        },
    }
}

fn input_mapping_from_arm(arm: Arm, cx: &mut ExtCtxt) -> Result<InputMapping, ()> {
    let Arm { pats, body, .. } = arm;

    let input = InputDefinition::from_pat(&pats[0], cx)?;
    let transition = Transition::from_expr(&body, cx)?;

    Ok(InputMapping { input, transition })
}

/// What happens when certain input is received
#[derive(Copy, Clone)]
enum Transition {
    State(State),
    Action(Action),
    StateAction(State, Action),
}

impl fmt::Debug for Transition {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Transition::State(state) => write!(f, "State({:?})", state)?,
            Transition::Action(action) => write!(f, "Action({:?})", action)?,
            Transition::StateAction(state, action) => {
                write!(f, "StateAction({:?}, {:?})", state, action)?;
            },
        }

        write!(f, " -> {:?}", self.pack_u8())
    }
}

impl Transition {
    // State is stored in the top 4 bits
    fn pack_u8(self) -> u8 {
        match self {
            Transition::State(state) => pack(state, Action::InvalidSequence),
            Transition::Action(action) => pack(State::Ground, action),
            Transition::StateAction(state, action) => pack(state, action),
        }
    }
}

impl Transition {
    fn from_expr(expr: &Expr, cx: &mut ExtCtxt) -> Result<Transition, ()> {
        match expr.node {
            ExprKind::Tup(ref tup_exprs) => {
                let mut action = None;
                let mut state = None;

                for tup_expr in tup_exprs {
                    if let ExprKind::Path(_, ref path) = tup_expr.node {
                        let path_str = path.to_string();
                        if path_str.starts_with('A') {
                            action = Some(action_from_str(&path_str).map_err(|_| {
                                cx.span_err(expr.span, "invalid action");
                            })?);
                        } else {
                            state = Some(state_from_str(&path_str).map_err(|_| {
                                cx.span_err(expr.span, "invalid state");
                            })?);
                        }
                    }
                }

                match (action, state) {
                    (Some(action), Some(state)) => Ok(Transition::StateAction(state, action)),
                    (None, Some(state)) => Ok(Transition::State(state)),
                    (Some(action), None) => Ok(Transition::Action(action)),
                    _ => {
                        cx.span_err(expr.span, "expected Action and/or State");
                        Err(())
                    },
                }
            },
            ExprKind::Path(_, ref path) => {
                // Path can be Action or State
                let path_str = path.to_string();

                if path_str.starts_with('A') {
                    let action = action_from_str(&path_str).map_err(|_| {
                        cx.span_err(expr.span, "invalid action");
                    })?;
                    Ok(Transition::Action(action))
                } else {
                    let state = state_from_str(&path_str).map_err(|_| {
                        cx.span_err(expr.span, "invalid state");
                    })?;

                    Ok(Transition::State(state))
                }
            },
            _ => {
                cx.span_err(expr.span, "expected Action and/or State");
                Err(())
            },
        }
    }
}

#[derive(Debug)]
enum InputDefinition {
    Specific(u8),
    Range { start: u8, end: u8 },
}

impl InputDefinition {
    fn from_pat(pat: &Pat, cx: &mut ExtCtxt) -> Result<InputDefinition, ()> {
        Ok(match pat.node {
            PatKind::Lit(ref lit_expr) => {
                InputDefinition::Specific(u8_lit_from_expr(&lit_expr, cx)?)
            },
            PatKind::Range(ref start_expr, ref end_expr) => InputDefinition::Range {
                start: u8_lit_from_expr(start_expr, cx)?,
                end: u8_lit_from_expr(end_expr, cx)?,
            },
            _ => {
                cx.span_err(pat.span, "expected literal or range expression");
                return Err(());
            },
        })
    }
}

#[derive(Debug)]
struct InputMapping {
    input: InputDefinition,
    transition: Transition,
}

#[derive(Debug)]
struct TableDefinition {
    state: State,
    mappings: Vec<InputMapping>,
}

fn parse_raw_definitions(
    definitions: Vec<TableDefinitionExprs>,
    cx: &mut ExtCtxt,
) -> Result<Vec<TableDefinition>, ()> {
    let mut out = Vec::new();

    for raw in definitions {
        let TableDefinitionExprs { state_expr, mapping_arms } = raw;
        let state = state_from_expr(state_expr, cx)?;

        let mut mappings = Vec::new();
        for arm in mapping_arms {
            mappings.push(input_mapping_from_arm(arm, cx)?);
        }

        out.push(TableDefinition { state, mappings })
    }

    Ok(out)
}

fn parse_table_definition<'a>(parser: &mut Parser<'a>) -> PResult<'a, TableDefinitionExprs> {
    let state_expr = parser.parse_expr()?;
    parser.expect(&Token::FatArrow)?;
    let mappings = parse_table_input_mappings(parser)?;

    Ok(TableDefinitionExprs { state_expr, mapping_arms: mappings })
}

fn parse_table_definition_list<'a>(
    parser: &mut Parser<'a>,
) -> PResult<'a, Vec<TableDefinitionExprs>> {
    let mut definitions = Vec::new();
    while parser.token != Token::Eof {
        definitions.push(parse_table_definition(parser)?);
        parser.eat(&Token::Comma);
    }

    Ok(definitions)
}

fn build_state_tables<T>(defs: T) -> [[u8; 256]; 8]
where
    T: AsRef<[TableDefinition]>,
{
    let mut result = [[0u8; 256]; 8];

    for def in defs.as_ref() {
        let state = def.state;
        let state = state as u8;
        let transitions = &mut result[state as usize];

        for mapping in &def.mappings {
            let trans = mapping.transition.pack_u8();
            match mapping.input {
                InputDefinition::Specific(idx) => {
                    transitions[idx as usize] = trans;
                },
                InputDefinition::Range { start, end } => {
                    for idx in start..end {
                        transitions[idx as usize] = trans;
                    }
                    transitions[end as usize] = trans;
                },
            }
        }
    }

    result
}

fn build_table_ast(cx: &mut ExtCtxt, sp: Span, table: [[u8; 256]; 8]) -> P<ast::Expr> {
    let table = table
        .iter()
        .map(|list| {
            let exprs = list.iter().map(|num| cx.expr_u8(sp, *num)).collect();
            cx.expr_vec(sp, exprs)
        })
        .collect();

    cx.expr_vec(sp, table)
}

fn expand_state_table<'cx>(
    cx: &'cx mut ExtCtxt,
    sp: Span,
    args: &[TokenTree],
) -> Box<dyn MacResult + 'cx> {
    macro_rules! ptry {
        ($pres:expr) => {
            match $pres {
                Ok(val) => val,
                Err(mut err) => {
                    err.emit();
                    return DummyResult::any(sp);
                },
            }
        };
    }

    // Parse the lookup spec
    let mut parser: Parser = cx.new_parser_from_tts(args);
    let definitions = ptry!(parse_table_definition_list(&mut parser));
    let definitions = match parse_raw_definitions(definitions, cx) {
        Ok(definitions) => definitions,
        Err(_) => return DummyResult::any(sp),
    };

    let table = build_state_tables(&definitions);
    let ast = build_table_ast(cx, sp, table);

    MacEager::expr(ast)
}
