Internal design details
=======================

Parsing
-------
The grammar for a valid katcp message can be recognised with a regular
expression, and the parser uses a corresponding deterministic finite state
machine (FSM). Each edge in the FSM has an associated action which indicates
what to do with the matching character.

.. tikz::
    :libs: automata, positioning, arrows

    \tikzset{
        ->,
        >=stealth',
        node distance=2cm,
        every state/.style={thick, fill=gray!10},
    }
    \node[state, initial] (Start) {};
    \node[state, below=of Start] (Empty) {};
    \node[state, right=of Start] (BeforeName) {};
    \node[state, right=of BeforeName] (Name) {};
    \node[state, above=of Name] (BeforeId) {};
    \node[state, right=of BeforeId] (Id) {};
    \node[state, right=of Id] (AfterId) {};
    \node[state, below=of AfterId] (BeforeArgument) {};
    \node[state, right=of BeforeArgument] (Argument) {};
    \node[state, above=of Argument] (ArgumentEscape) {};
    \node[state, accepting, below=of Argument] (EndOfLine) {};
    \draw
        (Start) edge[auto, bend left] node{SP} (Empty)
        (Start) edge[loop above] node{EOL} (Start)
        (Empty) edge[auto, bend left] node{EOL} (Start)
        (Empty) edge[loop below] node{SP} (Empty)
        (Start) edge[auto] node{!?\#} (BeforeName)
        (BeforeName) edge[auto] node{A--Za--z} (Name)
        (Name) edge[loop below] node{A--Za--z0--9-} (Name)
        (Name) edge[auto] node{[} (BeforeId)
        (Name) edge[auto] node{SP} (BeforeArgument)
        (Name) edge[auto, swap, bend right] node{EOL} (EndOfLine)
        (BeforeId) edge[auto] node{1--9} (Id)
        (Id) edge[loop above] node{0--9} (Id)
        (Id) edge[auto] node{]} (AfterId)
        (AfterId) edge[auto, pos=0.9] node{EOL} (EndOfLine)
        (AfterId) edge[auto] node{SP} (BeforeArgument)
        (BeforeArgument) edge[loop below] node{SP} (BeforeArgument)
        (BeforeArgument) edge[auto, pos=0.6] node{$\backslash$} (ArgumentEscape)
        (BeforeArgument) edge[auto, swap, pos=0.6] node{EOL} (EndOfLine)
        (BeforeArgument) edge[auto, pos=0.6, bend left] node{*} (Argument)
        (Argument) edge[auto, pos=0.3, bend left] node{SP} (BeforeArgument)
        (Argument) edge[auto, swap, pos=0.6, bend left] node{$\backslash$} (ArgumentEscape)
        (Argument) edge[auto, bend left] node{EOL} (EndOfLine)
        (Argument) edge[loop right] node{*} (Argument)
        (ArgumentEscape) edge[auto, bend left] node{ESC} (Argument)
    ;

The following abbreviations are used

ESC
    Characters valid after a backslash: ``@\\_0nret``
SP
    Space or tab
EOL
    Carriage return (``\r``) or newline (``\n``)
\*
    Any byte except for NUL (``\0``) or ESC (``\x1B``)

There are two additional states that are not shown: an error state, and an
additional error state for immediately after encountering an EOL. Encountering
any character that is not shown will transition to one of these states.

Acceleration
------------
Using a state machine makes it quite straightforward to build up a message
incrementally. Unfortunately, the initial implementation was quite slow,
because character of the message are appended to the storage one byte at a
time. An acceleration mechanism allows the input to be processed in chunks.
After taking one transition in the state machine, we scan over following
characters to find a maximal chunk that can be processed in one step. For
additional transitions to be combined with an initial one, they must:

- go to the same target state;

- take the same action;

- take an action that supports chunks;

- not start a new argument; and

- not go to a terminal state.

To speed up this test, each transition is accompanied by a 256-entry boolean
table that indicates which characters correspond to transitions satisfying
these conditions.

Given a complete message as input, this allows the following to be handled in
a single chunk:

- the message ID

- the name

- each argument.

Additionally, exceeding the maximum message length causes a transition to
error state, but this is not represented by the state machine. So the code to
scan for the chunk size has extra logic to stop the chunk if it would cross
that boundary.
