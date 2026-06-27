#!/usr/bin/env python3
"""Core linter module — facade over the runner registry."""

import os
from typing import Tuple, Optional
from .runners import registry


class JenkinsfileLinter:
    """Linter for validating Jenkinsfiles.

    Delegates to a ``ValidationRunner`` chosen via *runner*.
    New runners are auto-discovered from the ``runners/`` package.
    """

    @staticmethod
    def _pick(explicit: Optional[str], env_var: str) -> Optional[str]:
        """Return *explicit* if set, otherwise lookup *env_var* in the environment."""
        return explicit if explicit is not None else os.environ.get(env_var)

    def __init__(
        self,
        jenkins_url: Optional[str] = None,
        username: Optional[str] = None,
        token: Optional[str] = None,
        runner: str = "jenkins",
        **runner_kwargs,
    ):
        """Initialize the linter.

        Args:
            jenkins_url: Jenkins server URL (used by the ``jenkins`` runner).
            username: Jenkins username (used by the ``jenkins`` runner).
            token: Jenkins API token (used by the ``jenkins`` runner).
            runner: Validation backend name. Run ``jenkinsfilelint --help`` for
                the list of available runners. Defaults to ``"jenkins"``.
            **runner_kwargs: Additional keyword arguments forwarded to the
                runner's constructor (e.g. ``image=`` for the ``docker`` runner).
        """
        import inspect

        self._runner_name = runner

        # Collect kwargs that are intended for the runner,
        # preferring explicit arguments over environment variables
        kwargs = dict(runner_kwargs)

        jenkins_url = self._pick(jenkins_url, "JENKINS_URL")
        if jenkins_url is not None:
            kwargs.setdefault("jenkins_url", jenkins_url)

        username = self._pick(username, "JENKINS_USER")
        if username is not None:
            kwargs.setdefault("username", username)

        token = self._pick(token, "JENKINS_TOKEN")
        if token is not None:
            kwargs.setdefault("token", token)

        # Only pass kwargs that the runner's __init__ actually accepts
        try:
            runner_cls = registry.get_class(runner)
        except KeyError:
            available = ", ".join(registry.names())
            raise ValueError(
                f"Unknown runner: '{runner}'. Available runners: {available}"
            ) from None
        sig = inspect.signature(runner_cls.__init__)
        valid_params = set(sig.parameters.keys()) - {"self"}
        filtered_kwargs = {k: v for k, v in kwargs.items() if k in valid_params}

        self._runner = runner_cls(**filtered_kwargs)

    @property
    def runner_name(self) -> str:
        """Name of the active validation runner."""
        return self._runner_name

    def validate(self, jenkinsfile_path: str) -> Tuple[bool, str]:
        """Validate a Jenkinsfile using the configured runner.

        Args:
            jenkinsfile_path: Path to the Jenkinsfile.

        Returns:
            ``(is_valid, message)``
        """
        if not os.path.isfile(jenkinsfile_path):
            return False, f"File not found: {jenkinsfile_path}"
        return self._runner.validate(jenkinsfile_path)
