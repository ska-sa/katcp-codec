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

import enum
from dataclasses import dataclass
from typing import Optional, List, Union

from . import _lib


# Note: the values must correspond to those in message.rs
class MessageType(enum.Enum):
    REQUEST = 1
    REPLY = 2
    INFORM = 3


@dataclass
class Message:
    __slots__ = ["mtype", "name", "mid", "arguments"]

    mtype: MessageType
    name: bytes
    mid: Optional[int]
    arguments: List[bytes]


def _message_from_rust(message: Union[_lib.Message, ValueError]) -> Union[Message, ValueError]:
    if isinstance(message, Exception):
        return message
    else:
        return Message(
            MessageType(int(message.mtype)),
            message.name,
            message.mid,
            message.arguments
        )


class Parser:
    def __init__(self, max_line_length: int) -> None:
        self._parser = _lib.Parser(max_line_length)

    def append(self, data: bytes) -> List[Union[Message, ValueError]]:
        return [_message_from_rust(message) for message in self._parser.append(data)]
