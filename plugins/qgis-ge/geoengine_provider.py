# -*- coding: utf-8 -*-
"""
GeoEngine QGIS Processing Provider
Invokes the geoengine CLI directly to provide containerized geoprocessing tools
as QGIS Processing algorithms.
"""

import json
import os
import shutil
import subprocess
from typing import Any, Callable, Dict, List, Optional

from qgis.core import (
    QgsProcessingAlgorithm,
    QgsProcessingContext,
    QgsProcessingFeedback,
    QgsProcessingParameterRasterLayer,
    QgsProcessingParameterVectorLayer,
    QgsProcessingParameterString,
    QgsProcessingParameterNumber,
    QgsProcessingParameterBoolean,
    QgsProcessingParameterFile,
    QgsProcessingParameterFolderDestination,
    QgsProcessingParameterRasterDestination,
    QgsProcessingParameterVectorDestination,
    QgsProcessingParameterEnum,
    QgsProcessingProvider,
    QgsProcessingOutputRasterLayer,
    QgsProcessingOutputVectorLayer,
    QgsProcessingOutputFile,
)


# ---------------------------------------------------------------------------
# CLI Client
# ---------------------------------------------------------------------------

class GeoEngineCLIClient:
    """Client that invokes the geoengine CLI binary via subprocess."""

    def __init__(self):
        """
        Initialize the GeoEngine CLI client by locating the `geoengine` executable and storing its path on `self.binary`.
        
        Raises:
            FileNotFoundError: If the `geoengine` binary cannot be found on PATH or in known installation locations.
        """
        self.binary = self._find_binary()

    @staticmethod
    def _find_binary() -> str:
        """
        Locate the geoengine executable on the host system.
        
        Searches the current PATH (adds /usr/local/bin if absent) and then checks common user-install locations (~/.geoengine/bin and ~/.cargo/bin). Returns the absolute filesystem path to the first executable found.
        
        Returns:
            str: Absolute path to the geoengine executable.
        
        Raises:
            FileNotFoundError: If no executable is found; message suggests installing geoengine or adding it to PATH.
        """
        if "/usr/local/bin" not in os.environ["PATH"]:
            os.environ["PATH"] += ":/usr/local/bin"
        path = shutil.which('geoengine')
        if path:
            return path

        home = os.path.expanduser('~')
        for candidate in [
            os.path.join(home, '.geoengine', 'bin', 'geoengine'),
            os.path.join(home, '.cargo', 'bin', 'geoengine')
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
        Check that the configured geoengine binary responds and return its version.
        
        Returns:
            dict: A dictionary with keys 'status' (value 'healthy') and 'version' (the trimmed stdout from the geoengine binary).
        
        Raises:
            Exception: If the geoengine process exits with a non-zero status; the exception message includes the process stderr.
        """
        result = subprocess.run(
            [self.binary, '--version'],
            capture_output=True, text=True, timeout=10
        )
        if result.returncode != 0:
            raise Exception(f"geoengine version check failed: {result.stderr.strip()}")
        return {'status': 'healthy', 'version': result.stdout.strip()}

    def list_projects(self) -> List[Dict]:
        """
        Retrieve the list of available GeoEngine projects using the installed geoengine CLI.
        
        Returns:
            A list of project dictionaries parsed from the CLI's JSON output.
        
        Raises:
            Exception: If the geoengine CLI exits with a non-zero status (error message included).
            subprocess.TimeoutExpired: If the CLI call times out.
            json.JSONDecodeError: If the CLI produces invalid JSON.
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
        Retrieve the list of tools for a GeoEngine project by name.
        
        Parameters:
            name (str): The project name to query.
        
        Returns:
            List[Dict]: Parsed JSON value representing the project's tools (typically a list of tool metadata dictionaries).
        
        Raises:
            Exception: If the geoengine CLI exits with a non-zero code; the exception message includes the CLI stderr.
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
        Execute a GeoEngine project tool via the geoengine CLI, streaming CLI stderr to a callback and supporting cancellation.
        
        Parameters:
            project (str): Name of the GeoEngine project containing the tool.
            tool (str): Tool name to run within the project.
            inputs (Dict[str, Any]): Mapping of tool input names to values; None values are omitted.
            output_dir (Optional[str]): Path to a directory for tool outputs, passed to the CLI if provided.
            on_output (Optional[Callable[[str], None]]): Optional callback invoked for each non-empty stderr line produced by the CLI.
            is_cancelled (Optional[Callable[[], bool]]): Optional callable checked periodically; if it returns `True` the running CLI process is terminated and the call aborts.
        
        Returns:
            Dict: Parsed JSON result emitted on the CLI stdout, or a summary dict with `status`, `exit_code`, and `files` when no JSON is produced.
        
        Raises:
            Exception: If the tool exits with a non-zero exit code or if the job is cancelled by the caller.
        """
        cmd = [self.binary, 'project', 'run-tool', project, tool, '--json']
        if output_dir:
            cmd.extend(['--output-dir', output_dir])

        # Add input parameters as --input KEY=VALUE
        for key, value in inputs.items():
            if value is not None:
                cmd.extend(['--input', f'{key}={value}'])

        process = subprocess.Popen(
            cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True
        )

        try:
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

            stdout_data = process.stdout.read()
            if process.returncode == 0 and stdout_data.strip():
                return json.loads(stdout_data)
            elif process.returncode != 0:
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


# ---------------------------------------------------------------------------
# QGIS Processing Provider
# ---------------------------------------------------------------------------

class GeoEngineProvider(QgsProcessingProvider):
    """QGIS Processing provider for GeoEngine tools."""

    def __init__(self):
        super().__init__()
        self._algorithms = []

    def load(self) -> bool:
        """Load the provider."""
        self._algorithms = self._discover_algorithms()
        return True

    def unload(self):
        """Unload the provider."""
        pass

    def id(self) -> str:
        return 'geoengine'

    def name(self) -> str:
        return 'GeoEngine'

    def longName(self) -> str:
        return 'GeoEngine Containerized Geoprocessing'

    def icon(self):
        return QgsProcessingProvider.icon(self)

    def loadAlgorithms(self):
        """
        Register algorithms with the QGIS processing provider.
        """
        for alg in self._algorithms:
            self.addAlgorithm(alg)

    def _discover_algorithms(self) -> List[QgsProcessingAlgorithm]:
        """
        Discover available GeoEngine algorithms via the geoengine CLI.
        
        Attempts to query the local `geoengine` executable for projects and their tools and constructs
        a list of corresponding QgsProcessingAlgorithm instances. If discovery fails (for example,
        if the CLI binary is not found or an error occurs while querying), an empty list is returned.
        
        Returns:
            List[QgsProcessingAlgorithm]: Discovered algorithms, or an empty list if discovery failed.
        """
        algorithms = []

        try:
            client = GeoEngineCLIClient()
            projects = client.list_projects()

            for project in projects:
                tools = client.get_project_tools(project['name'])
                for tool in tools:
                    alg = GeoEngineAlgorithm(project['name'], tool)
                    algorithms.append(alg)

        except Exception:
            # Binary not found or other error, return empty list
            pass

        return algorithms


# ---------------------------------------------------------------------------
# QGIS Processing Algorithm
# ---------------------------------------------------------------------------


class GeoEngineAlgorithm(QgsProcessingAlgorithm):
    """Dynamic QGIS Processing algorithm for a GeoEngine tool."""

    def __init__(self, project_name: str, tool_info: Dict):
        super().__init__()
        self._project = project_name
        self._tool = tool_info
        self._tool_name = tool_info['name']
        self._inputs = tool_info.get('inputs', [])
        self._outputs = tool_info.get('outputs', [])

    def createInstance(self):
        return GeoEngineAlgorithm(self._project, self._tool)

    def name(self) -> str:
        return f"{self._project}_{self._tool_name}"

    def displayName(self) -> str:
        return self._tool.get('label', self._tool_name)

    def group(self) -> str:
        return self._project

    def groupId(self) -> str:
        return self._project

    def shortHelpString(self) -> str:
        return self._tool.get('description', '')

    def initAlgorithm(self, config=None):
        """Define algorithm parameters."""
        # Add input parameters
        for inp in self._inputs:
            param = self._create_parameter(inp, is_output=False)
            if param:
                self.addParameter(param)

        # Add output destination parameter
        self.addParameter(
            QgsProcessingParameterFolderDestination(
                'OUTPUT_DIR',
                'Output Directory'
            )
        )

        # Add output parameters
        for out in self._outputs:
            param = self._create_parameter(out, is_output=True)
            if param:
                self.addParameter(param)

    def _create_parameter(self, param_info: Dict, is_output: bool):
        """
        Create a QgsProcessingParameter (or destination) from tool parameter metadata.
        
        Parameters:
            param_info (Dict): Metadata describing the parameter. Recognized keys:
                - 'param_type' (str): type hint such as 'string', 'int', 'float', 'bool',
                  'raster', 'vector', 'file', or 'folder' (defaults to 'string').
                - 'name' (str): parameter identifier (required).
                - 'label' (str): human-readable label (defaults to name).
                - 'required' (bool): whether the parameter is required (defaults to True).
                - 'default': default value for the parameter, when applicable.
                - 'choices' (List[str]): enumeration options for string parameters.
            is_output (bool): If True, create an output/destination parameter; otherwise create an input parameter.
        
        Returns:
            QgsProcessingParameter or None: A concrete QgsProcessingParameter subclass appropriate for the metadata,
            or None when the requested output type is not supported.
        """
        param_type = param_info.get('param_type', 'string')
        name = param_info['name']
        label = param_info.get('label', name)
        required = param_info.get('required', True)
        default = param_info.get('default')
        options = param_info.get('choices', [])

        if is_output:
            if param_type == 'raster':
                return QgsProcessingParameterRasterDestination(name, label)
            elif param_type == 'vector':
                return QgsProcessingParameterVectorDestination(name, label)
            elif param_type == 'file':
                return QgsProcessingParameterFolderDestination(name, label)
            return None

        # Input parameters
        if param_type == 'raster':
            param = QgsProcessingParameterRasterLayer(name, label, optional=not required)
        elif param_type == 'vector':
            param = QgsProcessingParameterVectorLayer(name, label, optional=not required)
        elif param_type == 'string':
            if len(options) > 0:
                param = QgsProcessingParameterEnum(name, label, options, defaultValue=default, optional=not required, usesStaticStrings=True)
            else:
                param = QgsProcessingParameterString(name, label, defaultValue=default, optional=not required)
        elif param_type == 'int':
            param = QgsProcessingParameterNumber(
                name, label, type=QgsProcessingParameterNumber.Integer,
                defaultValue=default, optional=not required
            )
        elif param_type == 'float':
            param = QgsProcessingParameterNumber(
                name, label, type=QgsProcessingParameterNumber.Double,
                defaultValue=default, optional=not required
            )
        elif param_type == 'bool':
            param = QgsProcessingParameterBoolean(name, label, defaultValue=default or False, optional=not required)
        elif param_type == 'file':
            param = QgsProcessingParameterFile(name, label, optional=not required)
        elif param_type == 'folder':
            param = QgsProcessingParameterFile(
                name, label, behavior=QgsProcessingParameterFile.Folder, optional=not required
            )
        else:
            param = QgsProcessingParameterString(name, label, defaultValue=default, optional=not required)

        return param

    def processAlgorithm(
        self,
        parameters: Dict,
        context: QgsProcessingContext,
        feedback: QgsProcessingFeedback
    ) -> Dict:
        """
        Execute the configured GeoEngine tool via the geoengine CLI, stream CLI output to the QGIS feedback, and collect resulting output file paths.
        
        Streams stderr/stdout lines from the CLI to feedback.pushInfo and periodically checks feedback.isCanceled() to support user cancellation. Matches returned files to declared outputs by case-insensitive name containment.
        
        @returns Dictionary mapping output parameter names (and the 'OUTPUT_DIR' key) to their filesystem paths; outputs are matched case-insensitively against the tool's returned file names.
        """
        client = GeoEngineCLIClient()

        # Build inputs
        inputs = {}
        for inp in self._inputs:
            name = inp['name']
            if name in parameters:
                value = parameters[name]

                # Convert QGIS layer to file path
                if hasattr(value, 'source'):
                    value = value.source()
                elif hasattr(value, 'dataProvider'):
                    value = value.dataProvider().dataSourceUri()

                inputs[name] = str(value) if value else None

        # Get output directory
        output_dir = self.parameterAsString(parameters, 'OUTPUT_DIR', context)

        feedback.pushInfo(f"Running tool '{self._tool_name}' for project '{self._project}'...")

        # Run the tool synchronously, streaming container output
        result = client.run_tool(
            project=self._project,
            tool=self._tool_name,
            inputs=inputs,
            output_dir=output_dir,
            on_output=lambda line: feedback.pushInfo(line),
            is_cancelled=lambda: feedback.isCanceled(),
        )

        feedback.pushInfo("Tool completed successfully!")
        feedback.setProgress(100)

        # Build result dictionary
        results = {'OUTPUT_DIR': output_dir}

        output_files = result.get('files', [])
        feedback.pushInfo(f"Output files: {len(output_files)}")

        for out in self._outputs:
            name = out['name']
            # Try to find matching output file
            for f in output_files:
                if name.lower() in f['name'].lower():
                    results[name] = f['path']
                    break

        return results