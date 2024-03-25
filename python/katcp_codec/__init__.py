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
from typing import List, Optional, Union

from . import _lib


# Note: the values must correspond to those in message.rs
class MessageType(enum.Enum):
    """Type of katcp message."""

    REQUEST = 1
    REPLY = 2
    INFORM = 3


_MESSAGE_TYPE_MAP = {
    MessageType.REQUEST: _lib.MessageType.REQUEST,
    MessageType.REPLY: _lib.MessageType.REPLY,
    MessageType.INFORM: _lib.MessageType.INFORM,
}


@dataclass
class Message:
    """A katcp message."""

    __slots__ = ["mtype", "name", "mid", "arguments"]

    #: Message type
    mtype: MessageType
    #: Message name (must start with [A-Za-z] and contain only [A-Za-z0-9-])
    name: bytes
    #: Message ID (if specified, must be positive and less than 2**32)
    mid: Optional[int]
    #: Message arguments
    arguments: List[bytes]

    def __post_init__(self) -> None:
        if self.mid is not None and not 1 <= self.mid <= 2**31 - 1:
            raise OverflowError("Message ID must be in the range [1, 2**31 - 1)")

    def __bytes__(self) -> bytes:
        """Convert the message to its wire representation."""
        return bytes(_message_to_rust(self))


def _message_from_rust(
    message: Union[_lib.Message, ValueError]
) -> Union[Message, ValueError]:
    if isinstance(message, ValueError):
        return message
    else:
        return Message(
            MessageType(int(message.mtype)),
            message.name,
            message.mid,
            message.arguments,
        )


def _message_to_rust(message: Message) -> _lib.Message:
    return _lib.Message(
        _MESSAGE_TYPE_MAP[message.mtype],
        message.name,
        message.mid,
        message.arguments,
    )


class Parser:
    """Message parser.

    The parser accepts chunks of data from the wire (which need not be aligned
    to message boundaries) and returns whole messages as they are parsed.

    Parameters
    ----------
    max_line_length
        The maximum number of bytes in a message. Longer messages will not
        break the parser but will be reported as errors.
    """

    def __init__(self, max_line_length: int) -> None:
        self._parser = _lib.Parser(max_line_length)

    def append(self, data: bytes) -> List[Union[Message, ValueError]]:
        """Append new data to the parser.

        Returns
        -------
        messages
            Messages whose end was in the input data. Each message is either
            an instance of :class:`Message` if it was valid or
            :exc:`ValueError` if not.
        """
        return [_message_from_rust(message) for message in self._parser.append(data)]

    def reset(self) -> None:
        """Reset the parser to its initial state.

        This discards any incomplete message from the internal buffer.
        """
        self._parser.reset()

    @property
    def buffer_size(self) -> int:
        """Get the current size of the internal buffer.

        This contains the number of bytes received for the current partial
        message (if any). This does not indicate the memory usage, since
        those bytes may have already been parsed into internal structures.
        """
        return self._parser.buffer_size
