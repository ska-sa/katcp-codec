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

//! Tests that cut across modules

use proptest::prelude::*;

use crate::message::{Message, MessageType};
use crate::parse::Parser;

pub(crate) fn mtype_strategy() -> impl Strategy<Value = MessageType> {
    prop_oneof![
        Just(MessageType::Request),
        Just(MessageType::Reply),
        Just(MessageType::Inform),
    ]
}

pub(crate) fn name_strategy() -> impl Strategy<Value = Vec<u8>> {
    "[A-Za-z][-A-Za-z0-9]*".prop_map(|x| x.into_bytes())
}

pub(crate) fn mid_strategy() -> impl Strategy<Value = Option<i32>> {
    prop_oneof![Just(None), (1..0x7fffffffi32).prop_map(Some)]
}

pub(crate) fn arguments_strategy() -> impl Strategy<Value = Vec<Vec<u8>>> {
    prop::collection::vec(prop::collection::vec(0..255u8, 0..50), 0..50)
}

pub(crate) fn text_message_strategy() -> impl Strategy<Value = String> {
    r"[?!#][A-Za-z][-A-Za-z0-9]*(?:\[[1-9][0-9]{7}\])?(?:[ \t]+(?:[^\x00\x1B\r\n \t\\]|\\[rnet0_\\])+)*[ \t]*[\r\n]"
}

proptest! {
    /// Test that formatting a message then reparsing it gives the original message
    #[test]
    fn round_trip(
        mtype in mtype_strategy(),
        name in name_strategy(),
        mid in mid_strategy(),
        arguments in arguments_strategy()
    )
    {
        let message: Message<Vec<u8>, Vec<u8>> = Message::new(mtype, name, mid, arguments);
        let encoded = message.to_bytes();
        let mut parser = Parser::new(1000000000);
        let decoded: Vec<_> = parser.append(&encoded).collect();
        assert_eq!(decoded.len(), 1);
        let decoded = decoded[0].as_ref().unwrap();
        assert_eq!(*decoded, message);
    }
}
