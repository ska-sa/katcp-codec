Usage
=====

The library does not deal directly with network sockets. Instead, a
higher-level library like aiokatcp_ handles the networking and passes data
into and out of this library.

The katcp specification does not require any particular character encoding,
and nothing prevents message arguments from containing arbitrary binary data.
To reflect this, the APIs work with the Python :class:`bytes` type, leaving
character encoding and decoding to the user.

.. _aiokatcp: https://aiokatcp.readthedocs.io/

Parsing
-------
The parsing support is designed to work with an event-driven processing model,
where each received piece of data is inserted into the parser without
requiring alignment to message boundaries. If the parser receives a partial
message, it will remember the state and resume parsing a message when more
data arrives.

Start by creating a :class:`.Parser`. The constructor takes one parameter to
indicate the maximum number of bytes in a message. This can be set to a much
larger value than any messages you're expecting: the purpose is to prevent a
rogue message from consuming all the memory in the server.

As each piece of data arrives, pass it to :meth:`.Parser.append`. The return
value will be a list of new parsed messages. If any message couldn't be parsed
(for example, because it contained invalid characters or was formatted
incorrectly), the list will contain a :exc:`ValueError` rather than a
:class:`.Message`.

Formatting
----------
Construct a :class:`.Message`, then pass it to the :class:`bytes` constructor
to obtain the wire representation.
