import json
import os

from contextlib import contextmanager, closing, nullcontext
from typing import TextIO, Callable, Iterator, Dict, Any, ContextManager, NewType
from pathlib import Path


Hostname = NewType("Hostname", str)


@contextmanager
def stream_json(
    file: TextIO, close: bool = False
) -> Iterator[Callable[[Dict[str, Any]], None]]:
    """Streams JSON objects to a file-like object.

    Hack around the fact that json.dump doesn't allow streaming.

    If close is True, the file will be closed on exit.

    >>> with stream_json(open("test.json", "w")) as writer:
    ...   writer.write({"a": 1})
    ...   writer.write({"a": 1})
    >>> with open("test.json", "r") as f:
    ...   f.read() == '[\n{"a": 1},\n{"b": 2}\n]\n'
    True

    Args:
        f: file-like object (in str mode)
        close: if True, the f will be closed at the en
    Yields:
        callable that writes its argument to f
    """
    closer: ContextManager = closing(file) if close else nullcontext()
    with closer:
        file.write("[\n")
        first = True

        def writer(data):
            nonlocal first
            if not first:
                file.write(",\n")
            first = False
            json.dump(data, file)
            file.flush()

        yield writer
        file.write("\n]\n")


@contextmanager
def chdir(path: Path):
    old_cwd = os.getcwd()
    try:
        os.chdir(path)
        yield
    finally:
        os.chdir(old_cwd)
