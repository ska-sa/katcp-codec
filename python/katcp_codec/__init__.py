import katcp_codec._lib as _lib

def demo():
    parser = _lib.Parser(100000)
    msgs = parser.append(b"?hello[2] foo bar\n!response \\@\nbaz")
    print(msgs)
    print(msgs[0].message_type)
    print(msgs[0].name)
    print(msgs[0].id)
    print(msgs[0].arguments)
