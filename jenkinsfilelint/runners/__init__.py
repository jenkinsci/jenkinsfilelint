#!/usr/bin/env python3
"""Runner registry — auto-discovers all available ``ValidationRunner`` subclasses.

Every module in this package whose global names end with ``Runner`` (case-sensitive)
and whose class inherits from ``ValidationRunner`` is automatically discovered.

Usage::

    from jenkinsfilelint.runners import registry

    # List all registered runner names
    registry.names()       → ["docker", "jenkins"]

    # Get a runner instance by name
    runner = registry.get("docker")
    ok, msg = runner.validate("Jenkinsfile")
"""

from typing import Dict, List, Optional, Type
from .base import ValidationRunner


class _RunnerRegistry:
    """Internal registry that discovers and stores runner classes."""

    def __init__(self):
        self._runners: Dict[str, Type[ValidationRunner]] = {}
        self._discover()

    def _discover(self) -> None:
        """Import every public name in this package ending with ``Runner``."""
        import pkgutil
        import importlib
        import inspect

        pkg = importlib.import_module(__package__)  # jenkinsfilelint.runners
        for _, modname, _ in pkgutil.walk_packages(
            pkg.__path__,
            prefix=f"{__package__}.",
        ):
            try:
                mod = importlib.import_module(modname)
            except Exception:
                continue

            for name in dir(mod):
                if not name.endswith("Runner"):
                    continue
                obj = getattr(mod, name)
                if (
                    inspect.isclass(obj)
                    and issubclass(obj, ValidationRunner)
                    and obj is not ValidationRunner  # skip the ABC itself
                ):
                    self._runners[obj.name] = obj

    def names(self) -> List[str]:
        """Return sorted list of registered runner names."""
        return sorted(self._runners)

    def get(self, name: str, **kwargs) -> ValidationRunner:
        """Instantiate a runner by name.

        Args:
            name: Runner identifier (e.g. ``"jenkins"``, ``"docker"``).
            **kwargs: Forwarded to the runner's ``__init__``.

        Returns:
            A ready-to-use ``ValidationRunner`` instance.

        Raises:
            KeyError: When *name* is not registered.
        """
        cls = self._runners[name]
        return cls(**kwargs)

    def get_class(self, name: str) -> Type[ValidationRunner]:
        """Return the runner class without instantiating it.

        Args:
            name: Runner identifier.

        Returns:
            The ``ValidationRunner`` subclass.

        Raises:
            KeyError: When *name* is not registered.
        """
        return self._runners[name]

    def help_text(self) -> str:
        """Return formatted help for the ``--runner`` argument."""
        lines = [f"  {n:<20s} {self._runners[n].description}" for n in self.names()]
        return "\n".join(lines)


#: Singleton registry.
registry = _RunnerRegistry()
