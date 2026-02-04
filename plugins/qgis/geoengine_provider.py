# -*- coding: utf-8 -*-
"""
GeoEngine QGIS Processing Provider
Provides GeoEngine tools as QGIS Processing algorithms.
"""

import json
import time
import urllib.request
import urllib.error
from typing import Any, Dict, List, Optional

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
    QgsProcessingProvider,
    QgsProcessingOutputRasterLayer,
    QgsProcessingOutputVectorLayer,
    QgsProcessingOutputFile,
)


class GeoEngineClient:
    """Simple HTTP client for GeoEngine service."""

    def __init__(self, host: str = "localhost", port: int = 9876):
        self.base_url = f"http://{host}:{port}"

    def _request(self, method: str, endpoint: str, data: Optional[Dict] = None) -> Any:
        url = f"{self.base_url}{endpoint}"

        if data is not None:
            data_bytes = json.dumps(data).encode('utf-8')
            req = urllib.request.Request(
                url, data=data_bytes, method=method,
                headers={'Content-Type': 'application/json'}
            )
        else:
            req = urllib.request.Request(url, method=method)

        try:
            with urllib.request.urlopen(req, timeout=30) as response:
                return json.loads(response.read().decode('utf-8'))
        except urllib.error.HTTPError as e:
            error_body = e.read().decode('utf-8')
            try:
                error_json = json.loads(error_body)
                raise Exception(error_json.get('error', str(e)))
            except json.JSONDecodeError:
                raise Exception(f"HTTP {e.code}: {error_body}")
        except urllib.error.URLError as e:
            raise Exception(f"Cannot connect to GeoEngine: {e.reason}")

    def health_check(self) -> Dict:
        return self._request('GET', '/api/health')

    def list_projects(self) -> List[Dict]:
        return self._request('GET', '/api/projects')

    def get_project_tools(self, name: str) -> List[Dict]:
        return self._request('GET', f'/api/projects/{name}/tools')

    def submit_job(self, project: str, tool: str, inputs: Dict, output_dir: Optional[str] = None) -> str:
        data = {'project': project, 'tool': tool, 'inputs': inputs}
        if output_dir:
            data['output_dir'] = output_dir
        response = self._request('POST', '/api/jobs', data)
        return response['id']

    def get_job_status(self, job_id: str) -> Dict:
        return self._request('GET', f'/api/jobs/{job_id}')

    def get_job_output(self, job_id: str) -> List[Dict]:
        response = self._request('GET', f'/api/jobs/{job_id}/output')
        return response.get('files', [])


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
        """Load algorithms into the provider."""
        for alg in self._algorithms:
            self.addAlgorithm(alg)

    def _discover_algorithms(self) -> List[QgsProcessingAlgorithm]:
        """Discover algorithms from GeoEngine service."""
        algorithms = []

        try:
            client = GeoEngineClient()
            projects = client.list_projects()

            for project in projects:
                tools = client.get_project_tools(project['name'])
                for tool in tools:
                    alg = GeoEngineAlgorithm(project['name'], tool)
                    algorithms.append(alg)

        except Exception as e:
            # Service not available, return empty list
            pass

        return algorithms


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
        """Create a QGIS parameter from tool parameter info."""
        param_type = param_info.get('param_type', 'string')
        name = param_info['name']
        label = param_info.get('label', name)
        required = param_info.get('required', True)
        default = param_info.get('default')

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
        """Execute the algorithm."""
        client = GeoEngineClient()

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

        feedback.pushInfo(f"Submitting job to GeoEngine...")
        feedback.pushInfo(f"Project: {self._project}")
        feedback.pushInfo(f"Tool: {self._tool_name}")

        # Submit job
        job_id = client.submit_job(
            project=self._project,
            tool=self._tool_name,
            inputs=inputs,
            output_dir=output_dir
        )

        feedback.pushInfo(f"Job submitted: {job_id}")

        # Poll for completion
        while True:
            if feedback.isCanceled():
                feedback.pushInfo("Cancelling job...")
                # TODO: Cancel job via API
                return {}

            status = client.get_job_status(job_id)
            current_status = status['status']

            feedback.pushInfo(f"Status: {current_status}")

            if current_status == 'completed':
                feedback.pushInfo("Job completed successfully!")
                break
            elif current_status == 'failed':
                raise Exception(f"Job failed: {status.get('error', 'Unknown error')}")
            elif current_status == 'cancelled':
                raise Exception("Job was cancelled")

            # Update progress (estimate based on time)
            feedback.setProgress(50)
            time.sleep(5)

        # Get outputs
        output_files = client.get_job_output(job_id)
        feedback.pushInfo(f"Output files: {len(output_files)}")

        # Build result dictionary
        results = {'OUTPUT_DIR': output_dir}

        for out in self._outputs:
            name = out['name']
            # Try to find matching output file
            for f in output_files:
                if name.lower() in f['name'].lower():
                    results[name] = f['path']
                    break

        return results
