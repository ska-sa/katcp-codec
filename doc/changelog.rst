Changelog
=========

0.2.0
-----
- Support buffer objects in :meth:`.Parser.append`. As a result, separate
  Python wheels are now built for versions older than 3.11.
- Significantly speed up parsing when there are escapes in message arguments
  or when a message is split across several calls to :meth:`.Parser.append`.
- Switch to use ruff_ for linting and uv_ for locking Python dependencies.
- Update to newer versions of various dependencies.

.. _ruff: https://docs.astral.sh/ruff/
.. _uv: https://docs.astral.sh/uv/

0.1.0
-----
- Fix a typo in an error message
- Update dependencies

0.1.0b2
-------
First release

0.1.0b1
-------
Never released, due to packaging problems
