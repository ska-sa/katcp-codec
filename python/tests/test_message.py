################################################################################
# Copyright (c) 2025, National Research Foundation (SARAO)
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

import gc
import weakref

from katcp_codec._lib import Message, MessageType


class Dummy:
    pass


def test_message_gc():
    msg = Message(MessageType.REQUEST, b"hello", None, [b"arg1"])
    # Create cyclic garbage by appending the message to its own arguments.
    msg.arguments.append(msg)
    # msg itself doesn't support weakrefs, so add another object to the list
    # to detect cleanup
    canary = Dummy()
    msg.arguments.append(canary)
    weak_canary = weakref.ref(canary)
    del canary
    # Ensure that the arguments don't get prematurely garbage collected.
    for i in range(5):  # One gc.collect isn't always enough
        gc.collect()
    assert weak_canary() is not None

    del msg
    # Ensure that canary gets garbage collected.
    for i in range(5):
        gc.collect()
    assert weak_canary() is None
