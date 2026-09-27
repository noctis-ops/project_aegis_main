'use client';

import { useState, useEffect } from 'react';

interface HFTStatus {
  running: boolean;
  symbols: string[];
  uptime: string;
  lastUpdate: string;
  metrics: {
    eventsProcessed: number;
    latency: string;
    memoryUsage: string;
    cpuUsage: string;
  };
}

export default function HFTDashboard() {
  const [status, setStatus] = useState<HFTStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [symbols, setSymbols] = useState('BTCUSDT,ETHUSDT');

  useEffect(() => {
    fetchStatus();
  }, []);

  const fetchStatus = async () => {
    try {
      setLoading(true);
      const response = await fetch('/api/hft/status');
      const data = await response.json();
      
      if (data.success) {
        setStatus(data.status);
      } else {
        setError(data.error || 'Failed to fetch status');
      }
    } catch (err) {
      setError('Failed to connect to HFT system');
    } finally {
      setLoading(false);
    }
  };

  const startHFT = async () => {
    try {
      const symbolList = symbols.split(',').map(s => s.trim()).filter(s => s);
      const response = await fetch('/api/hft/start', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ symbols: symbolList }),
      });
      
      const data = await response.json();
      
      if (data.success) {
        alert(data.message);
        fetchStatus();
      } else {
        alert(data.error || 'Failed to start HFT system');
      }
    } catch (err) {
      alert('Failed to start HFT system');
    }
  };

  const stopHFT = async () => {
    try {
      const response = await fetch('/api/hft/stop', {
        method: 'POST',
      });
      
      const data = await response.json();
      
      if (data.success) {
        alert(data.message);
        fetchStatus();
      } else {
        alert(data.error || 'Failed to stop HFT system');
      }
    } catch (err) {
      alert('Failed to stop HFT system');
    }
  };

  if (loading) {
    return (
      <div className="min-h-screen bg-gray-50 flex items-center justify-center">
        <div className="text-xl">Loading HFT system status...</div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-gray-50 py-12">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <div className="text-center mb-12">
          <h1 className="text-3xl font-bold text-gray-900 mb-2">Project AEGIS - Layer 1</h1>
          <p className="text-lg text-gray-600">High-Frequency Trading System Dashboard</p>
          <div className="mt-4">
            <a 
              href="/hft-dashboard/ar" 
              className="inline-flex items-center px-4 py-2 border border-gray-300 shadow-sm text-sm font-medium rounded-md text-gray-700 bg-white hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500"
            >
              العربية
            </a>
          </div>
        </div>

        {error && (
          <div className="bg-red-50 border-l-4 border-red-400 p-4 mb-8">
            <div className="flex">
              <div className="flex-shrink-0">
                <svg className="h-5 w-5 text-red-400" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor">
                  <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clipRule="evenodd" />
                </svg>
              </div>
              <div className="ml-3">
                <p className="text-sm text-red-700">{error}</p>
              </div>
            </div>
          </div>
        )}

        {/* Control Panel */}
        <div className="bg-white shadow rounded-lg mb-8">
          <div className="px-4 py-5 sm:p-6">
            <h2 className="text-lg font-medium text-gray-900 mb-4">Control Panel</h2>
            
            <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
              <div className="md:col-span-2">
                <label htmlFor="symbols" className="block text-sm font-medium text-gray-700 mb-1">
                  Trading Symbols
                </label>
                <input
                  type="text"
                  id="symbols"
                  value={symbols}
                  onChange={(e) => setSymbols(e.target.value)}
                  className="shadow-sm focus:ring-indigo-500 focus:border-indigo-500 block w-full sm:text-sm border-gray-300 rounded-md px-3 py-2"
                  placeholder="BTCUSDT,ETHUSDT"
                />
                <p className="mt-1 text-sm text-gray-500">Comma-separated list of symbols to trade</p>
              </div>
              
              <div className="flex items-end space-x-3">
                <button
                  onClick={startHFT}
                  className="inline-flex items-center px-4 py-2 border border-transparent text-sm font-medium rounded-md shadow-sm text-white bg-green-600 hover:bg-green-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-green-500"
                >
                  Start HFT Engine
                </button>
                
                <button
                  onClick={stopHFT}
                  className="inline-flex items-center px-4 py-2 border border-transparent text-sm font-medium rounded-md shadow-sm text-white bg-red-600 hover:bg-red-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-red-500"
                >
                  Stop HFT Engine
                </button>
              </div>
            </div>
          </div>
        </div>

        {/* Status Panel */}
        {status && (
          <div className="bg-white shadow rounded-lg overflow-hidden">
            <div className="px-4 py-5 sm:px-6 border-b border-gray-200">
              <h2 className="text-lg font-medium text-gray-900">System Status</h2>
            </div>
            
            <div className="px-4 py-5 sm:p-6">
              <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                <div>
                  <h3 className="text-md font-medium text-gray-900 mb-3">General Information</h3>
                  <dl className="grid grid-cols-1 gap-x-4 gap-y-2">
                    <div className="flex justify-between">
                      <dt className="text-sm font-medium text-gray-500">Status</dt>
                      <dd className="text-sm text-gray-900">
                        <span className={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium ${
                          status.running ? 'bg-green-100 text-green-800' : 'bg-red-100 text-red-800'
                        }`}>
                          {status.running ? 'Running' : 'Stopped'}
                        </span>
                      </dd>
                    </div>
                    <div className="flex justify-between">
                      <dt className="text-sm font-medium text-gray-500">Uptime</dt>
                      <dd className="text-sm text-gray-900">{status.uptime}</dd>
                    </div>
                    <div className="flex justify-between">
                      <dt className="text-sm font-medium text-gray-500">Last Update</dt>
                      <dd className="text-sm text-gray-900">{new Date(status.lastUpdate).toLocaleTimeString()}</dd>
                    </div>
                    <div className="flex justify-between">
                      <dt className="text-sm font-medium text-gray-500">Symbols</dt>
                      <dd className="text-sm text-gray-900">{status.symbols.join(', ')}</dd>
                    </div>
                  </dl>
                </div>
                
                <div>
                  <h3 className="text-md font-medium text-gray-900 mb-3">Performance Metrics</h3>
                  <dl className="grid grid-cols-1 gap-x-4 gap-y-2">
                    <div className="flex justify-between">
                      <dt className="text-sm font-medium text-gray-500">Events Processed</dt>
                      <dd className="text-sm text-gray-900">{status.metrics.eventsProcessed.toLocaleString()}</dd>
                    </div>
                    <div className="flex justify-between">
                      <dt className="text-sm font-medium text-gray-500">Latency</dt>
                      <dd className="text-sm text-gray-900">{status.metrics.latency}</dd>
                    </div>
                    <div className="flex justify-between">
                      <dt className="text-sm font-medium text-gray-500">Memory Usage</dt>
                      <dd className="text-sm text-gray-900">{status.metrics.memoryUsage}</dd>
                    </div>
                    <div className="flex justify-between">
                      <dt className="text-sm font-medium text-gray-500">CPU Usage</dt>
                      <dd className="text-sm text-gray-900">{status.metrics.cpuUsage}</dd>
                    </div>
                  </dl>
                </div>
              </div>
              
              <div className="mt-6">
                <button
                  onClick={fetchStatus}
                  className="inline-flex items-center px-3 py-2 border border-gray-300 shadow-sm text-sm leading-4 font-medium rounded-md text-gray-700 bg-white hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500"
                >
                  Refresh Status
                </button>
              </div>
            </div>
          </div>
        )}

        {/* Architecture Overview */}
        <div className="mt-8 bg-white shadow rounded-lg overflow-hidden">
          <div className="px-4 py-5 sm:px-6 border-b border-gray-200">
            <h2 className="text-lg font-medium text-gray-900">Architecture Overview</h2>
          </div>
          
          <div className="px-4 py-5 sm:p-6">
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
              <div className="border border-gray-200 rounded-lg p-4">
                <h3 className="font-medium text-gray-900 mb-2">Network & Transport</h3>
                <ul className="text-sm text-gray-600 space-y-1">
                  <li>• WebSocket Connections</li>
                  <li>• Heartbeat Monitoring</li>
                  <li>• REST API Client</li>
                </ul>
              </div>
              
              <div className="border border-gray-200 rounded-lg p-4">
                <h3 className="font-medium text-gray-900 mb-2">Order Book Management</h3>
                <ul className="text-sm text-gray-600 space-y-1">
                  <li>• Local Order Book (LOB)</li>
                  <li>• Snapshot Synchronization</li>
                  <li>• Update Validation</li>
                </ul>
              </div>
              
              <div className="border border-gray-200 rounded-lg p-4">
                <h3 className="font-medium text-gray-900 mb-2">Event Processing</h3>
                <ul className="text-sm text-gray-600 space-y-1">
                  <li>• Real-time Ring Buffers</li>
                  <li>• Zero-lock Communication</li>
                  <li>• Backpressure Handling</li>
                </ul>
              </div>
              
              <div className="border border-gray-200 rounded-lg p-4">
                <h3 className="font-medium text-gray-900 mb-2">Fault Tolerance</h3>
                <ul className="text-sm text-gray-600 space-y-1">
                  <li>• Sequence Gap Detection</li>
                  <li>• Automatic Recovery</li>
                  <li>• Emergency Procedures</li>
                </ul>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}