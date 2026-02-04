# -*- coding: utf-8 -*-
"""
GeoEngine Client - Python client library for the GeoEngine proxy service.
Used by ArcGIS Pro toolbox and can be used standalone.
"""

import json
import urllib.request
import urllib.error
from typing import Dict, List, Optional, Any


class GeoEngineClient:
    """Client for communicating with the GeoEngine proxy service."""

    def __init__(self, host: str = "localhost", port: int = 9876):
        """
        Initialize the GeoEngine client.

        Args:
            host: Hostname of the GeoEngine service
            port: Port number of the GeoEngine service
        """
        self.base_url = f"http://{host}:{port}"

    def _request(self, method: str, endpoint: str, data: Optional[Dict] = None) -> Any:
        """Make an HTTP request to the service."""
        url = f"{self.base_url}{endpoint}"

        if data is not None:
            data_bytes = json.dumps(data).encode('utf-8')
            req = urllib.request.Request(
                url,
                data=data_bytes,
                method=method,
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
            raise Exception(f"Cannot connect to GeoEngine service: {e.reason}")

    def health_check(self) -> Dict:
        """
        Check the health of the GeoEngine service.

        Returns:
            Dict with status, version, and uptime information
        """
        return self._request('GET', '/api/health')

    def list_projects(self) -> List[Dict]:
        """
        List all registered projects.

        Returns:
            List of project summaries with name, version, path, and tools_count
        """
        return self._request('GET', '/api/projects')

    def get_project(self, name: str) -> Dict:
        """
        Get detailed information about a project.

        Args:
            name: Project name

        Returns:
            Full project configuration
        """
        return self._request('GET', f'/api/projects/{name}')

    def get_project_tools(self, name: str) -> List[Dict]:
        """
        Get the list of tools available in a project.

        Args:
            name: Project name

        Returns:
            List of tool definitions with inputs and outputs
        """
        return self._request('GET', f'/api/projects/{name}/tools')

    def submit_job(
        self,
        project: str,
        tool: str,
        inputs: Dict[str, Any],
        output_dir: Optional[str] = None
    ) -> str:
        """
        Submit a new processing job.

        Args:
            project: Project name
            tool: Tool name to execute
            inputs: Input parameters as key-value pairs
            output_dir: Directory to write output files

        Returns:
            Job ID
        """
        data = {
            'project': project,
            'tool': tool,
            'inputs': inputs,
        }
        if output_dir:
            data['output_dir'] = output_dir

        response = self._request('POST', '/api/jobs', data)
        return response['id']

    def get_job_status(self, job_id: str) -> Dict:
        """
        Get the status of a job.

        Args:
            job_id: Job ID

        Returns:
            Job details including status, timestamps, and error info
        """
        return self._request('GET', f'/api/jobs/{job_id}')

    def list_jobs(self, all_jobs: bool = False) -> List[Dict]:
        """
        List jobs.

        Args:
            all_jobs: If True, include completed/failed jobs

        Returns:
            List of job summaries
        """
        endpoint = '/api/jobs'
        if all_jobs:
            endpoint += '?all=true'
        return self._request('GET', endpoint)

    def cancel_job(self, job_id: str) -> Dict:
        """
        Cancel a running or queued job.

        Args:
            job_id: Job ID

        Returns:
            Updated job status
        """
        return self._request('DELETE', f'/api/jobs/{job_id}')

    def get_job_output(self, job_id: str) -> List[Dict]:
        """
        Get the output files from a completed job.

        Args:
            job_id: Job ID

        Returns:
            List of output files with name, path, and size
        """
        response = self._request('GET', f'/api/jobs/{job_id}/output')
        return response.get('files', [])

    def wait_for_job(
        self,
        job_id: str,
        poll_interval: float = 5.0,
        timeout: Optional[float] = None,
        callback: Optional[callable] = None
    ) -> Dict:
        """
        Wait for a job to complete.

        Args:
            job_id: Job ID
            poll_interval: Seconds between status checks
            timeout: Maximum seconds to wait (None for no timeout)
            callback: Optional callback function called with status updates

        Returns:
            Final job status

        Raises:
            TimeoutError: If timeout is exceeded
            Exception: If job fails
        """
        import time

        start_time = time.time()

        while True:
            status = self.get_job_status(job_id)

            if callback:
                callback(status)

            if status['status'] == 'completed':
                return status
            elif status['status'] == 'failed':
                raise Exception(f"Job failed: {status.get('error', 'Unknown error')}")
            elif status['status'] == 'cancelled':
                raise Exception("Job was cancelled")

            if timeout and (time.time() - start_time) > timeout:
                raise TimeoutError(f"Job did not complete within {timeout} seconds")

            time.sleep(poll_interval)


# Convenience functions for standalone use
def run_tool(
    project: str,
    tool: str,
    inputs: Dict[str, Any],
    output_dir: Optional[str] = None,
    wait: bool = True,
    host: str = "localhost",
    port: int = 9876
) -> Dict:
    """
    Run a GeoEngine tool and optionally wait for completion.

    Args:
        project: Project name
        tool: Tool name
        inputs: Input parameters
        output_dir: Output directory
        wait: If True, wait for job to complete
        host: GeoEngine service host
        port: GeoEngine service port

    Returns:
        Job status or output info
    """
    client = GeoEngineClient(host, port)
    job_id = client.submit_job(project, tool, inputs, output_dir)

    if wait:
        return client.wait_for_job(job_id)
    else:
        return {'id': job_id, 'status': 'queued'}


if __name__ == '__main__':
    # Example usage
    import sys

    client = GeoEngineClient()

    try:
        health = client.health_check()
        print(f"Service Status: {health['status']}")
        print(f"Version: {health['version']}")

        projects = client.list_projects()
        print(f"\nRegistered Projects: {len(projects)}")
        for p in projects:
            print(f"  - {p['name']} ({p['tools_count']} tools)")

    except Exception as e:
        print(f"Error: {e}")
        sys.exit(1)
