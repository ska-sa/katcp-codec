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

from typing import Final, List, Optional, Union

from typing_extensions import Buffer

# Not inherited from enum.Enum, because Pyo3 doesn't provide full compatibility.
class MessageType:
    REQUEST: Final[MessageType] = ...
    REPLY: Final[MessageType] = ...
    INFORM: Final[MessageType] = ...

    def __int__(self) -> int: ...

class Message:
    mtype: MessageType
    name: bytes
    mid: Optional[int]
    arguments: List[bytes]

    # TODO: does it have to be List, or would Iterable work?
    def __init__(
        self,
        mtype: MessageType,
        name: bytes,
        mid: Optional[int],
        arguments: List[bytes],
    ) -> None: ...
    def __bytes__(self) -> bytes: ...

class Parser:
    def __init__(self, max_line_length: int) -> None: ...
    def append(self, data: Buffer) -> List[Union[Message, ValueError]]: ...
    def reset(self) -> None: ...
    @property
    def buffer_size(self) -> int: ...
