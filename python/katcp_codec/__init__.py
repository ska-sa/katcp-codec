import katcp_codec._lib as _lib

def demo():
    parser = _lib.Parser(100000)
    msgs = parser.append(b"?hello[1] foo bar\n!response \\@\nbaz")
    print(msgs)
    print(msgs[0].id)

    parser2 = _lib.Parser(15)
    msgs = parser2.append(b"?hello[1] foo bar\n!response \\@\nbaz")
    print(msgs)
