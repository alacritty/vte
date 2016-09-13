use syntex::Registry;

use syntex_syntax::ast::{ExprKind, MetaItem, Arm, Expr, PatKind, LitKind, Pat};
use syntex_syntax::codemap::Span;
use syntex_syntax::ext::base::{Annotatable, ExtCtxt, MacEager, MacResult, DummyResult};
use syntex_syntax::ext::build::AstBuilder;
use syntex_syntax::parse::token::{Token, InternedString, DelimToken};
use syntex_syntax::parse::parser::Parser;
use syntex_syntax::parse::PResult;
use syntex_syntax::ptr::P;
use syntex_syntax::tokenstream::TokenTree;

use definitions::{State, Action};

pub fn register(registry: &mut Registry) {
    registry.add_macro("state_table", expand_state_table);
}

fn state_from_str<S>(s: &S) -> Result<State, ()>
    where S: AsRef<str>
{
    Ok(match s.as_ref() {
        "State::Anywhere" => State::Anywhere,
        "State::CsiEntry" => State::CsiEntry,
        "State::CsiIgnore" => State::CsiIgnore,
        "State::CsiIntermediate" => State::CsiIntermediate,
        "State::CsiParam" => State::CsiParam,
        "State::DcsEntry" => State::DcsEntry,
        "State::DcsIgnore" => State::DcsIgnore,
        "State::DcsIntermediate" => State::DcsIntermediate,
        "State::DcsParam" => State::DcsParam,
        "State::DcsPassthrough" => State::DcsPassthrough,
        "State::Escape" => State::Escape,
        "State::EscapeIntermediate" => State::EscapeIntermediate,
        "State::Ground" => State::Ground,
        "State::OscString" => State::OscString,
        "State::SosPmApcString" => State::SosPmApcString,
        _ => return Err(())
    })
}

fn action_from_str<S>(s: &S) -> Result<Action, ()>
    where S: AsRef<str>
{
    Ok(match s.as_ref() {
        "Action::None" => Action::None,
        "Action::Clear" => Action::Clear,
        "Action::Collect" => Action::Collect,
        "Action::CsiDispatch" => Action::CsiDispatch,
        "Action::EscDispatch" => Action::EscDispatch,
        "Action::Execute" => Action::Execute,
        "Action::Hook" => Action::Hook,
        "Action::Ignore" => Action::Ignore,
        "Action::OscEnd" => Action::OscEnd,
        "Action::OscPut" => Action::OscPut,
        "Action::OscStart" => Action::OscStart,
        "Action::Param" => Action::Param,
        "Action::Print" => Action::Print,
        "Action::Put" => Action::Put,
        "Action::Unhook" => Action::Unhook,
        _ => return Err(())
    })
}

fn parse_table_input_mappings<'a>(parser: &mut Parser<'a>) -> PResult<'a, Vec<Arm>> {
    // Must start on open brace
    try!(parser.expect(&Token::OpenDelim(DelimToken::Brace)));

    let mut arms: Vec<Arm> = Vec::new();
    while parser.token != Token::CloseDelim(DelimToken::Brace) {
        match parser.parse_arm() {
            Ok(arm) => arms.push(arm),
            Err(mut e) => {
                // Recover by skipping to the end of the block.
                return Err(e);
            }
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
        ExprKind::Path(ref _qself, ref path) => {
            path.to_string()
        },
        _ => {
            cx.span_err(expr.span, "expected State");
            return Err(())
        }
    };

    state_from_str(&s).map_err(|_| {
        cx.span_err(expr.span, "expected State");
        ()
    })
}

fn u8_lit_from_expr(expr: &Expr, cx: &mut ExtCtxt) -> Result<u8, ()> {
    static MSG: &'static str = "expected u8 int literal";

    match expr.node {
        ExprKind::Lit(ref lit) => {
            match lit.node {
                LitKind::Int(val, _) => {
                    Ok(val as u8)
                },
                _ => {
                    cx.span_err(lit.span, MSG);
                    return Err(());
                }
            }
        },
        _ => {
            cx.span_err(expr.span, MSG);
            return Err(());
        }
    }
}

fn input_mapping_from_arm(arm: Arm, cx: &mut ExtCtxt) -> Result<InputMapping, ()> {
    let Arm { pats, body, .. } = arm;

    let input = try!(InputDefinition::from_pat(&pats[0], cx));
    let transition = try!(Transition::from_expr(&body, cx));

    Ok(InputMapping {
        input: input,
        transition: transition,
    })
}

/// What happens when certain input is received
#[derive(Debug)]
enum Transition {
    State(State),
    Action(Action),
    StateAction(State, Action),
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
                            action = Some(try!(action_from_str(&path_str)
                                               .map_err(|_| {
                                                   cx.span_err(expr.span, "invalid action");
                                               })));
                        } else {
                            state = Some(try!(state_from_str(&path_str)
                                               .map_err(|_| {
                                                   cx.span_err(expr.span, "invalid state");
                                               })));
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
                    }
                }
            },
            ExprKind::Path(_, ref path) => {
                // Path can be Action or State
                let path_str = path.to_string();

                if path_str.starts_with('A') {
                    let action = try!(action_from_str(&path_str)
                                     .map_err(|_| {
                                         cx.span_err(expr.span, "invalid action");
                                     }));
                    Ok(Transition::Action(action))
                } else {
                    let state = try!(state_from_str(&path_str)
                                     .map_err(|_| {
                                         cx.span_err(expr.span, "invalid state");
                                     }));

                    Ok(Transition::State(state))
                }
            },
            _ => {
                cx.span_err(expr.span, "expected Action and/or State");
                Err(())
            }
        }
    }
}

#[derive(Debug)]
enum InputDefinition {
    Specific(u8),
    Range { start: u8, end: u8 }
}

impl InputDefinition {
    fn from_pat(pat: &Pat, cx: &mut ExtCtxt) -> Result<InputDefinition, ()> {
        Ok(match pat.node {
            PatKind::Lit(ref lit_expr) => {
                InputDefinition::Specific(try!(u8_lit_from_expr(&lit_expr, cx)))
            },
            PatKind::Range(ref start_expr, ref end_expr) => {
                InputDefinition::Range {
                    start: try!(u8_lit_from_expr(start_expr, cx)),
                    end: try!(u8_lit_from_expr(end_expr, cx)),
                }
            },
            _ => {
                cx.span_err(pat.span, "expected literal or range expression");
                return Err(())
            }
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
    cx: &mut ExtCtxt
) -> Result<Vec<TableDefinition>, ()> {
    let mut out = Vec::new();

    for raw in definitions {
        let TableDefinitionExprs { state_expr, mapping_arms } = raw;
        let state = try!(state_from_expr(state_expr, cx));

        let mut mappings = Vec::new();
        for arm in mapping_arms {
            mappings.push(try!(input_mapping_from_arm(arm, cx)));
        }

        out.push(TableDefinition {
            state: state,
            mappings: mappings,
        })
    }

    Ok(out)
}

fn parse_table_definition<'a>(parser: &mut Parser<'a>) -> PResult<'a, TableDefinitionExprs> {
    let state_expr = try!(parser.parse_expr());
    try!(parser.expect(&Token::FatArrow));
    let mappings = try!(parse_table_input_mappings(parser));

    Ok(TableDefinitionExprs {
        state_expr: state_expr,
        mapping_arms: mappings
    })
}

fn parse_table_definition_list<'a>(parser: &mut Parser<'a>)
    -> PResult<'a, Vec<TableDefinitionExprs>>
{
    let mut definitions = Vec::new();
    while parser.token != Token::Eof {
        definitions.push(try!(parse_table_definition(parser)));
        parser.eat(&Token::Comma);
    }

    Ok(definitions)
}

fn expand_state_table<'cx>(
    cx: &'cx mut ExtCtxt,
    sp: Span,
    args: &[TokenTree])
    -> Box<MacResult + 'cx>
{
    macro_rules! ptry {
        ($pres:expr) => {
            match $pres {
                Ok(val) => val,
                Err(mut err) => {
                    err.emit();
                    return DummyResult::any(sp);
                }
            }
        }
    }

    // Parse the lookup spec
    let mut parser: Parser = cx.new_parser_from_tts(args);
    let definitions = ptry!(parse_table_definition_list(&mut parser));
    let definitions = match parse_raw_definitions(definitions, cx) {
        Ok(definitions) => definitions,
        Err(_) => return DummyResult::any(sp),
    };

    println!("definitions: {:?}", definitions);

    panic!("End of current implementation, go write some more :)");
}
