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
    Request = 1,
    #[pyo3(name = "REPLY")]
    Reply = 2,
    #[pyo3(name = "INFORM")]
    Inform = 3,
}

pub struct Message<'data> {
    pub mtype: MessageType,
    pub name: Cow<'data, [u8]>,
    pub mid: Option<i32>,
    pub arguments: Vec<Cow<'data, [u8]>>,
}

impl<'data> Message<'data> {
    pub fn new(
        mtype: MessageType,
        name: Cow<'data, [u8]>,
        mid: Option<i32>,
        arguments: Vec<Cow<'data, [u8]>>,
    ) -> Self {
        Self {
            mtype,
            name,
            mid,
            arguments,
        }
    }
}
