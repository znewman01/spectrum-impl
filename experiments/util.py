"""Utilities.

A module named "util" is a clear indication that you haven't thought hard enough
about how to organize your code.
"""
import asyncio
import json

from contextlib import contextmanager, closing, nullcontext
from typing import (
    TextIO,
    Callable,
    Iterator,
    Dict,
    Any,
    ContextManager,
    NewType,
    TypeVar,
    Awaitable,
)


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


# Pylint bug: https://github.com/PyCQA/pylint/issues/3401
K = TypeVar("K")  # pylint: disable=invalid-name
V = TypeVar("V")  # pylint: disable=invalid-name


async def gather_dict(tasks: Dict[K, Awaitable[V]]) -> Dict[K, V]:
    """Gather {keys:awaitables} into {keys:(results of those awaitables)}."""

    async def do_it(key, coro):
        return key, await coro

    return dict(
        await asyncio.gather(*(do_it(key, coro) for key, coro in tasks.items()))
    )
