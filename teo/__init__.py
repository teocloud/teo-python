from .teo import App, Namespace, Model, Field, Relation, Property, Enum, EnumMember, Response, Request, ReadOnlyHeaderMap, ReadWriteHeaderMap, HandlerMatch, HandlerGroup, RequestCtx, ObjectId, Range, OptionVariant, EnumVariant, File, Pipeline
from typing import TypeVar, Union
from signal import signal, SIGINT
from sys import exit

T = TypeVar('T')

Enumerable = Union[T, list[T]]

signal(SIGINT, lambda _, __: exit(0))
