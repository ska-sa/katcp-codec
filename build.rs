/* Copyright (c) 2024, National Research Foundation (SARAO)
 *
 * Licensed under the BSD 3-Clause License (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy
 * of the License at
 *
 *   https://opensource.org/licenses/BSD-3-Clause
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use enum_map::{enum_map, EnumMap};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::rc::Rc;

use katcp_codec_fsm::{Action, MessageType, State};

/// (state, char) entry in the state machine.
///
/// This is separate from the definition in src/parse.rs because it needs to
/// construct the fast_table at runtime.
#[derive(Clone, Default, Eq, PartialEq, Hash)]
struct Entry {
    /// Action to apply
    action: Action,
    /// Next state
    state: State,
    /// Whether to create a new argument before applying the action
    create_argument: bool,
    /// Following characters that can be merged into the action
    fast_table: Option<Rc<EnumMap<u8, bool>>>,
}

impl Entry {
    /// Construct a new entry.
    ///
    /// The fast_table is omitted; these are filled in later.
    fn new_full(action: Action, state: State, create_argument: bool) -> Self {
        Self {
            action,
            state,
            create_argument,
            fast_table: None,
        }
    }

    /// Construct a new entry that does not start a new argument.
    fn new(action: Action, state: State) -> Self {
        Self::new_full(action, state, false)
    }

    /// Construct an entry that signals an error.
    fn error() -> Self {
        Self::new(Action::Error, State::Error)
    }
}

/// Generic helper for building the transition table for one state.
///
/// The callback is invoked for every [u8] value. The rules for `' '`
/// and `\n` are copied over those for `\t` and `\r` respectively.
fn make_table(callback: impl Fn(u8) -> Entry) -> EnumMap<u8, Entry> {
    let mut table = EnumMap::default();
    for ch in 0..=255u8 {
        table[ch] = callback(ch);
    }
    // Simplify the callers by applying some generic rules
    if table[b'\n'].state == State::Error {
        table[b'\n'].state = State::ErrorEndOfLine;
    }
    assert!(matches!(
        table[b'\n'].state,
        State::EndOfLine | State::ErrorEndOfLine | State::Start
    ));
    table[b'\t'] = table[b' '].clone();
    table[b'\r'] = table[b'\n'].clone();
    table
}

/// Create a transition table for an error state.
fn make_error() -> EnumMap<u8, Entry> {
    make_table(|_| Entry::error())
}

/// Create the transition table for [State::Start].
fn make_start() -> EnumMap<u8, Entry> {
    make_table(|ch| match ch {
        b' ' => Entry::new(Action::Nothing, State::Empty),
        b'?' => Entry::new(Action::SetType(MessageType::Request), State::BeforeName),
        b'!' => Entry::new(Action::SetType(MessageType::Reply), State::BeforeName),
        b'#' => Entry::new(Action::SetType(MessageType::Inform), State::BeforeName),
        b'\n' => Entry::new(Action::ResetLineLength, State::Start),
        _ => Entry::error(),
    })
}

/// Create the transition table for [State::Empty].
fn make_empty() -> EnumMap<u8, Entry> {
    make_table(|ch| match ch {
        b' ' => Entry::new(Action::Nothing, State::Empty),
        b'\n' => Entry::new(Action::ResetLineLength, State::Start),
        _ => Entry::error(),
    })
}

/// Create the transition table for [State::BeforeName].
fn make_before_name() -> EnumMap<u8, Entry> {
    make_table(|ch| match ch {
        b'A'..=b'Z' | b'a'..=b'z' => Entry::new(Action::Name, State::Name),
        _ => Entry::error(),
    })
}

/// Create the transition table for [State::Name].
fn make_name() -> EnumMap<u8, Entry> {
    make_table(|ch| match ch {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' => Entry::new(Action::Name, State::Name),
        b' ' => Entry::new(Action::Nothing, State::BeforeArgument),
        b'[' => Entry::new(Action::Nothing, State::BeforeId),
        b'\n' => Entry::new(Action::Nothing, State::EndOfLine),
        _ => Entry::error(),
    })
}

/// Create the transition table for [State::BeforeId].
fn make_before_id() -> EnumMap<u8, Entry> {
    make_table(|ch| match ch {
        b'1'..=b'9' => Entry::new(Action::Id, State::Id),
        _ => Entry::error(),
    })
}

/// Create the transition table for [State::Id].
fn make_id() -> EnumMap<u8, Entry> {
    make_table(|ch| match ch {
        b'0'..=b'9' => Entry::new(Action::Id, State::Id),
        b']' => Entry::new(Action::Nothing, State::AfterId),
        _ => Entry::error(),
    })
}

/// Create the transition table for [State::AfterId].
fn make_after_id() -> EnumMap<u8, Entry> {
    make_table(|ch| match ch {
        b' ' => Entry::new(Action::Nothing, State::BeforeArgument),
        b'\n' => Entry::new(Action::Nothing, State::EndOfLine),
        _ => Entry::error(),
    })
}

/// Create the transition table for [State::BeforeArgument] or [State::Argument].
///
/// If `create_argument` is true, a non-space character will start a new
/// argument. This should be done for [State::BeforeArgument].
fn make_argument(create_argument: bool) -> EnumMap<u8, Entry> {
    make_table(|ch| match ch {
        b' ' => Entry::new(Action::Nothing, State::BeforeArgument),
        b'\n' => Entry::new(Action::Nothing, State::EndOfLine),
        b'\\' => Entry::new_full(Action::Nothing, State::ArgumentEscape, create_argument),
        b'\0' | b'\x1B' => Entry::error(),
        _ => Entry::new_full(Action::Argument, State::Argument, create_argument),
    })
}

/// Create the transition table for [State::ArgumentEscape].
fn make_argument_escape() -> EnumMap<u8, Entry> {
    make_table(|ch| match ch {
        b'@' => Entry::new(Action::Nothing, State::Argument),
        b'\\' => Entry::new(Action::ArgumentEscaped(b'\\'), State::Argument),
        b'_' => Entry::new(Action::ArgumentEscaped(b' '), State::Argument),
        b'0' => Entry::new(Action::ArgumentEscaped(b'\0'), State::Argument),
        b'n' => Entry::new(Action::ArgumentEscaped(b'\n'), State::Argument),
        b'r' => Entry::new(Action::ArgumentEscaped(b'\r'), State::Argument),
        b'e' => Entry::new(Action::ArgumentEscaped(b'\x1B'), State::Argument),
        b't' => Entry::new(Action::ArgumentEscaped(b'\t'), State::Argument),
        _ => Entry::error(),
    })
}

/// Fill in the [Entry::fast_table] slots.
fn build_fast_tables(table: &mut EnumMap<State, EnumMap<u8, Entry>>) {
    type ActionDisc = std::mem::Discriminant<Action>;

    let mut cache: HashMap<(State, ActionDisc), Rc<EnumMap<u8, bool>>> = HashMap::new();

    // Rust borrowing rules complicate this looping. We need to mutate
    // the table, which we can't do if we're borrowing it for iteration.
    let states: Vec<State> = table
        .iter()
        .map(|(state, _)| state)
        .filter(|state| !state.is_terminal())
        .collect();
    for src_state in states {
        for ch in 0..=255u8 {
            let entry = &table[src_state][ch];
            if entry.state.is_terminal() || !entry.action.is_mergeable() {
                continue;
            }
            let state = entry.state;
            let key = (state, std::mem::discriminant(&entry.action));
            // Lifetime of `entry` ends here, leaving `table` accessible

            let fast_table = cache.entry(key).or_insert_with(|| {
                let mut result = EnumMap::default();
                for ch2 in 0..=255u8 {
                    let entry = &table[state][ch2];
                    result[ch2] = entry.state == state
                        && std::mem::discriminant(&entry.action) == key.1
                        && !entry.create_argument;
                }
                Rc::new(result)
            });
            if fast_table.values().any(|x| *x) {
                table[src_state][ch].fast_table = Some(fast_table.clone());
            }
        }
    }
}

/// Build the parser table.
fn parser_table() -> EnumMap<State, EnumMap<u8, Entry>> {
    let mut table = enum_map! {
        State::Start => make_start(),
        State::Empty => make_empty(),
        State::BeforeName => make_before_name(),
        State::Name => make_name(),
        State::BeforeId => make_before_id(),
        State::Id => make_id(),
        State::AfterId => make_after_id(),
        State::BeforeArgument => make_argument(true),
        State::Argument => make_argument(false),
        State::ArgumentEscape => make_argument_escape(),
        State::Error => make_error(),
        State::EndOfLine => make_error(),
        State::ErrorEndOfLine => make_error(),
    };
    build_fast_tables(&mut table);
    table
}

fn write_parser_tables(w: &mut impl Write) -> Result<(), std::io::Error> {
    let table = parser_table();

    // First write each unique fast table.
    let mut fast_table_names: HashMap<Rc<EnumMap<u8, bool>>, String> = HashMap::new();
    let mut counter = 0;
    for row in table.values() {
        for entry in row.values() {
            if let Some(fast) = &entry.fast_table {
                let old_counter = counter;
                let name = fast_table_names.entry(fast.clone()).or_insert_with(|| {
                    let name = format!("FAST_TABLE{counter}");
                    counter += 1;
                    name.to_owned()
                });
                if counter != old_counter {
                    // This is a new entry
                    writeln!(w, "const {name}: EnumMap<u8, bool> = EnumMap::from_array([")?;
                    for i in 0..=255u8 {
                        writeln!(w, "    {},", fast[i])?;
                    }
                    writeln!(w, "]);")?;
                }
            }
        }
    }

    // Now write the entries.
    writeln!(
        w,
        "pub(crate) const PARSER_TABLE: EnumMap<State, EnumMap<u8, Entry>> = EnumMap::from_array(["
    )?;
    for row in table.values() {
        writeln!(w, "    EnumMap::from_array([")?;
        for entry in row.values() {
            writeln!(w, "        Entry {{")?;
            writeln!(w, "            action: Action::{:?},", entry.action)?;
            writeln!(w, "            state: State::{:?},", entry.state)?;
            writeln!(
                w,
                "            create_argument: {:?},",
                entry.create_argument
            )?;
            if let Some(fast_table) = &entry.fast_table {
                let name = &fast_table_names[fast_table];
                writeln!(w, "            fast_table: Some(&{name}),")?;
            } else {
                writeln!(w, "            fast_table: None,")?;
            }
            writeln!(w, "        }},")?;
        }
        writeln!(w, "    ]),")?;
    }
    writeln!(w, "]);")?;

    Ok(())
}

fn escape(c: u8) -> u8 {
    match c {
        b'\r' => b'r',
        b'\n' => b'n',
        b'\t' => b't',
        b'\x1B' => b'e',
        b'\0' => b'0',
        b'\\' => b'\\',
        b' ' => b'_',
        _ => 0, // Marker for not needing an escape
    }
}

fn write_format_tables(w: &mut impl Write) -> Result<(), std::io::Error> {
    writeln!(
        w,
        "pub(crate) const ESCAPE_SYMBOL: EnumMap<u8, u8> = EnumMap::from_array(["
    )?;
    for i in 0..=255u8 {
        let value = escape(i);
        writeln!(w, "    {value},")?;
    }
    writeln!(w, "]);")?;

    writeln!(
        w,
        "pub(crate) const ESCAPE_FLAG: EnumMap<u8, bool> = EnumMap::from_array(["
    )?;
    for i in 0..=255u8 {
        let value = escape(i) != 0;
        writeln!(w, "    {value},")?;
    }
    writeln!(w, "]);")?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let out_dir = std::env::var_os("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir);
    let tables_path = out_path.join("tables.rs");
    let mut tables_writer = fs::File::create(tables_path)?;

    write_parser_tables(&mut tables_writer)?;
    write_format_tables(&mut tables_writer)?;
    drop(tables_writer);

    println!("cargo:rerun-if-changed=build.rs");
    Ok(())
}
