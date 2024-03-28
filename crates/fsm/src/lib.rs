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

use enum_map::Enum;

/// Type of katcp message
#[cfg_attr(
    feature = "pyo3",
    pyo3::pyclass(module = "katcp_codec._lib", rename_all = "SCREAMING_SNAKE_CASE")
)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum MessageType {
    Request = 1,
    Reply = 2,
    Inform = 3,
}

/// State in the state machine
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash, Enum)]
pub enum State {
    /// Initial state
    Start,
    /// Seen whitespace, so this can only legally be a blank line
    Empty,
    /// Seen the type, haven't started the name
    BeforeName,
    /// Middle of the name
    Name,
    /// After [ in message ID
    BeforeId,
    /// Middle of the message ID
    Id,
    /// After the ] terminating the message ID
    AfterId,
    /// Seen some whitespace, haven't started the next argument yet
    BeforeArgument,
    /// Middle of an argument, not following a backslash
    Argument,
    /// Seen a backslash in an argument
    ArgumentEscape,
    /// Invalid character seen, waiting for the end-of-line
    #[default]
    Error,
    /// Terminal state for a valid line
    EndOfLine,
    /// Terminal state for an invalid line
    ErrorEndOfLine,
}

impl State {
    /// Whether this state corresponds to an end of line.
    pub fn is_terminal(&self) -> bool {
        matches!(self, State::EndOfLine | State::ErrorEndOfLine)
    }
}

/// Transition action in the state machine
#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
pub enum Action {
    /// No action needed (e.g. skipping whitespace, or an error)
    #[default]
    Nothing,
    /// Append the current character to the name
    Name,
    /// Append a digit to the message ID
    Id,
    /// Append the current character to the argument
    Argument,
    /// Append a specific character to the argument
    ArgumentEscaped(u8),
    /// Set the message type
    SetType(MessageType),
    /// Set line_length back to 0 (after empty message)
    ResetLineLength,
    /// Set an error message
    Error,
}

impl Action {
    pub fn is_mergeable(&self) -> bool {
        matches!(
            self,
            Action::Nothing | Action::Name | Action::Id | Action::Argument | Action::Error
        )
    }
}
