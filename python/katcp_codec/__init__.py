from dataclasses import dataclass

import katcp_codec._lib as _lib
from katcp_codec._lib import MessageType

@dataclass(slots=True)
class Message:
    message_type: MessageType
    name: bytes
    id: int | None
    arguments: list[bytes]


def _message_from_raw(raw: _lib.Message | ValueError) -> Message | ValueError:
    if isinstance(raw, _lib.Message):
        return Message(raw.message_type, raw.name, raw.id, raw.arguments)
    else:
        return raw


class Parser:
    def __init__(self, max_line_length: int) -> None:
        self._parser = _lib.Parser(max_line_length)

    def append(self, data: bytes) -> list[Message | ValueError]:
        raw = self._parser.append(data)
        return [_message_from_raw(msg) for msg in raw]
