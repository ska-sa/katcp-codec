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

use pyo3::prelude::*;

use std::borrow::Cow;

#[pyclass(module = "katcp_codec._lib")]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum MessageType {
    #[pyo3(name = "REQUEST")]
    Request,
    #[pyo3(name = "REPLY")]
    Reply,
    #[pyo3(name = "INFORM")]
    Inform,
}

pub struct Message<'data> {
    pub message_type: MessageType,
    pub name: Cow<'data, [u8]>,
    pub id: Option<i32>,
    pub arguments: Vec<Cow<'data, [u8]>>,
}

impl<'data> Message<'data> {
    pub fn new(
        message_type: MessageType,
        name: Cow<'data, [u8]>,
        id: Option<i32>,
        arguments: Vec<Cow<'data, [u8]>>,
    ) -> Self {
        Self {
            message_type,
            name,
            id,
            arguments,
        }
    }
}
