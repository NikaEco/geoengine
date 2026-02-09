# -*- coding: utf-8 -*-
"""
GeoEngine Client - CLI-based client library for GeoEngine.
Invokes the geoengine binary directly via subprocess (no HTTP proxy required).
Used by ArcGIS Pro toolbox and can be used standalone.
"""

import json
import os
import shutil
import subprocess
from typing import Any, Callable, Dict, List, Optional


class GeoEngineClient:
    """Client that invokes the geoengine CLI binary via subprocess."""

    def __init__(self):
        """
        Create a GeoEngineClient and locate the `geoengine` CLI binary.
        
        Attempts to locate the `geoengine` executable and stores its filesystem path on `self.binary`.
        Raises FileNotFoundError if the binary cannot be found.
        """
        self.binary = self._find_binary()

    @staticmethod
    def _find_binary() -> str:
        """
        Locate the geoengine executable binary on the system.
        
        Searches the system PATH first, then checks common fallback locations under the user's home directory
        (e.g., ~/.geoengine/bin/geoengine and ~/.cargo/bin/geoengine).
        
        Returns:
            path (str): Absolute path to the geoengine executable.
        
        Raises:
            FileNotFoundError: If the geoengine executable cannot be found or is not executable.
        """
        path = shutil.which('geoengine')
        if path:
            return path

        # Fallback locations
        home = os.path.expanduser('~')
        for candidate in [
            os.path.join(home, '.geoengine', 'bin', 'geoengine'),
            os.path.join(home, '.cargo', 'bin', 'geoengine'),
        ]:
            if os.path.isfile(candidate) and os.access(candidate, os.X_OK):
                return candidate

        raise FileNotFoundError(
            "geoengine binary not found. "
            "Install it from https://github.com/NikaGeospatial/geoengine "
            "or ensure it is on your PATH."
        )

    def version_check(self) -> Dict:
        """
        Verify the installed geoengine CLI is usable and return its reported version.
        
        Returns:
            A dict with keys:
              - 'status': the string 'healthy' when the CLI ran successfully.
              - 'version': the version string reported by the geoengine CLI.
        
        Raises:
            Exception: If the geoengine process exits with a non-zero code; the exception message contains the CLI stderr.
        """
        result = subprocess.run(
            [self.binary, '--version'],
            capture_output=True, text=True, timeout=10
        )
        if result.returncode != 0:
            raise Exception(f"geoengine version check failed: {result.stderr.strip()}")
        return {
            'status': 'healthy',
            'version': result.stdout.strip(),
        }

    def list_projects(self) -> List[Dict]:
        """
        List all registered GeoEngine projects.
        
        Returns:
            A list of project summary dictionaries. Each dictionary contains at least the keys
            `name`, `path`, and `tools_count`.
        
        Raises:
            Exception: If the geoengine CLI invocation fails.
        """
        result = subprocess.run(
            [self.binary, 'project', 'list', '--json'],
            capture_output=True, text=True, timeout=30
        )
        if result.returncode != 0:
            raise Exception(f"Failed to list projects: {result.stderr.strip()}")
        return json.loads(result.stdout)

    def get_project_tools(self, name: str) -> List[Dict]:
        """
        Retrieve tool definitions for the specified project.
        
        Parameters:
            name (str): Project name to query.
        
        Returns:
            A list of tool definition dictionaries parsed from the CLI's JSON output.
        
        Raises:
            Exception: If the geoengine CLI fails to retrieve tools; the exception message will contain the CLI stderr.
        """
        result = subprocess.run(
            [self.binary, 'project', 'tools', name],
            capture_output=True, text=True, timeout=30
        )
        if result.returncode != 0:
            raise Exception(f"Failed to get tools for '{name}': {result.stderr.strip()}")
        return json.loads(result.stdout)

    def run_tool(
        self,
        project: str,
        tool: str,
        inputs: Dict[str, Any],
        output_dir: Optional[str] = None,
        on_output: Optional[Callable[[str], None]] = None,
        is_cancelled: Optional[Callable[[], bool]] = None,
    ) -> Dict:
        """
        Execute a GeoEngine project tool synchronously and stream its realtime output.
        
        Parameters:
            project (str): Name of the project containing the tool.
            tool (str): Name of the tool to execute.
            inputs (Dict[str, Any]): Mapping of tool input names to values; entries with value None are skipped.
            output_dir (Optional[str]): Optional directory where the tool should write output files.
            on_output (Optional[Callable[[str], None]]): Optional callback invoked for each line of realtime output (stderr).
            is_cancelled (Optional[Callable[[], bool]]): Optional callback that should return True to request job cancellation.
        
        Returns:
            Dict: Parsed JSON result from the tool on success, or a summary dict when no structured output is produced.
                Typical successful JSON is returned as-is. When the tool exits with code 0 but produces no stdout,
                returns {'status': 'completed', 'exit_code': 0, 'files': []}.
        
        Raises:
            Exception: If the tool process exits with a non-zero code, or if the job is cancelled via is_cancelled.
        """
        cmd = [self.binary, 'project', 'run-tool', project, tool, '--json']
        if output_dir:
            cmd.extend(['--output-dir', output_dir])

        # Add input parameters as --input KEY=VALUE
        for key, value in inputs.items():
            if value is not None:
                cmd.extend(['--input', f'{key}={value}'])

        process = subprocess.Popen(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

        try:
            # Read stderr line-by-line for real-time progress
            for line in iter(process.stderr.readline, ''):
                if line:
                    stripped = line.rstrip('\n')
                    if on_output and stripped:
                        on_output(stripped)
                if is_cancelled and is_cancelled():
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
                    raise Exception("Job cancelled by user")

            process.wait()

            # Read structured JSON result from stdout
            stdout_data = process.stdout.read()
            if process.returncode == 0 and stdout_data.strip():
                return json.loads(stdout_data)
            elif process.returncode != 0:
                # Try to parse JSON error from stdout
                if stdout_data.strip():
                    try:
                        return json.loads(stdout_data)
                    except json.JSONDecodeError:
                        pass
                raise Exception(f"Tool exited with code {process.returncode}")
            else:
                return {'status': 'completed', 'exit_code': 0, 'files': []}
        finally:
            if process.stdout:
                process.stdout.close()
            if process.stderr:
                process.stderr.close()


# Convenience function for standalone use
def run_tool(
    project: str,
    tool: str,
    inputs: Dict[str, Any],
    output_dir: Optional[str] = None,
    on_output: Optional[Callable[[str], None]] = None,
) -> Dict:
    """
    Execute a GeoEngine tool via the GeoEngine CLI and return the tool's result.
    
    Parameters:
        project (str): Name of the project containing the tool.
        tool (str): Name of the tool to run.
        inputs (Dict[str, Any]): Mapping of tool input names to values.
        output_dir (Optional[str]): Directory where the tool should write outputs, if supported.
        on_output (Optional[Callable[[str], None]]): Optional callback invoked for each line of progress/output emitted by the tool (receives the line as a string).
    
    Returns:
        Dict: The tool run result. On success this is the parsed JSON output produced by the CLI; if the CLI produces no JSON it will be a dict such as {'status': 'completed', 'exit_code': 0, 'files': [...]}.
    
    Raises:
        Exception: If the CLI invocation fails, returns a non-zero exit code, or the job is cancelled.
    """
    client = GeoEngineClient()
    return client.run_tool(project, tool, inputs, output_dir, on_output=on_output)


if __name__ == '__main__':
    import sys

    client = GeoEngineClient()

    try:
        info = client.version_check()
        print(f"GeoEngine: {info['version']}")

        projects = client.list_projects()
        print(f"\nRegistered Projects: {len(projects)}")
        for p in projects:
            print(f"  - {p['name']} ({p.get('tools_count', 0)} tools)")

    except Exception as e:
        print(f"Error: {e}")
        sys.exit(1)