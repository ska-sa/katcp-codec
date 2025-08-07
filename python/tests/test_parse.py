################################################################################
# Copyright (c) 2024, National Research Foundation (SARAO)
#
# Licensed under the BSD 3-Clause License (the "License"); you may not use
# this file except in compliance with the License. You may obtain a copy
# of the License at
#
#   https://opensource.org/licenses/BSD-3-Clause
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
################################################################################

from typing import List

import pytest

from katcp_codec import Message, MessageType, Parser


@pytest.fixture
def max_line_length() -> int:
    return 1000


@pytest.fixture
def parser(max_line_length) -> Parser:
    return Parser(max_line_length)


@pytest.mark.parametrize(
    "data, messages",
    [
        (
            b"?hello world\n",
            [Message(MessageType.REQUEST, b"hello", None, [b"world"])],
        ),
        (
            b"!reply[2] \\@\n",
            [Message(MessageType.REPLY, b"reply", 2, [b""])],
        ),
        (
            b"\n\t\n#inform\t\\n\xFF\r!hello \n",
            [
                Message(MessageType.INFORM, b"inform", None, [b"\n\xFF"]),
                Message(MessageType.REPLY, b"hello", None, []),
            ],
        ),
    ],
)
@pytest.mark.parametrize("type_", [bytes, memoryview])
def test_success(
    parser: Parser, data: bytes, messages: List[Message], type_: type
) -> None:
    assert parser.append(type_(data)) == messages


def test_buffer_size(parser: Parser) -> None:
    assert parser.buffer_size == 0
    parser.append(b"?hello world")
    assert parser.buffer_size == 12
    parser.append(b"\n")
    assert parser.buffer_size == 0
    parser.append(b"invalid format")
    assert parser.buffer_size == 14
    parser.append(b"\nmore")
    assert parser.buffer_size == 4
    parser.reset()
    assert parser.buffer_size == 0


def test_reset(parser: Parser) -> None:
    parser.append(b"?query ")
    parser.reset()
    assert parser.append(b"!reply\n") == [
        Message(MessageType.REPLY, b"reply", None, [])
    ]
