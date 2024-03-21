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

import pytest

from katcp_codec import Message, MessageType, _lib


@pytest.mark.parametrize("mid", [-1, 0, -(2**31) - 1, 2**31])
def test_bad_mid(mid: int) -> None:
    with pytest.raises(OverflowError):
        Message(MessageType.REQUEST, b"hello", mid, [])
    # Test the underlying Rust type, to ensure it also protects against this
    with pytest.raises(OverflowError):
        _lib.Message(_lib.MessageType.REQUEST, b"hello", mid, [])


@pytest.mark.parametrize(
    "message, encoding",
    [
        (
            Message(MessageType.REQUEST, b"hello", None, [b"foo", b"bar"]),
            b"?hello foo bar\n",
        ),
        (
            Message(
                MessageType.REPLY, b"test-mid", 2147483647, [b"", b"\r\n\t\x1B\0\\ "]
            ),
            b"!test-mid[2147483647] \\@ \\r\\n\\t\\e\\0\\\\\\_\n",
        ),
    ],
)
def test_success(message: Message, encoding: bytes) -> None:
    assert bytes(message) == encoding
