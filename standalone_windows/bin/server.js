const express = require('express');
const http = require('http');
const socketIo = require('socket.io');
const path = require('path');
const fs = require('fs');

// Create data directory if it doesn't exist
const dataDir = path.join(__dirname, '../data');
if (!fs.existsSync(dataDir)) {
    fs.mkdirSync(dataDir, { recursive: true });
}

// Create logs directory if it doesn't exist
const logsDir = path.join(__dirname, '../logs');
if (!fs.existsSync(logsDir)) {
    fs.mkdirSync(logsDir, { recursive: true });
}

// Create backup directory if it doesn't exist
const backupDir = path.join(__dirname, '../backup');
if (!fs.existsSync(backupDir)) {
    fs.mkdirSync(backupDir, { recursive: true });
}

// Simple in-memory data store
let systemState = {
  running: false,
  symbols: ['BTCUSDT', 'ETHUSDT'],
  uptime: '0h 0m',
  lastUpdate: new Date().toISOString(),
  metrics: {
    eventsProcessed: 0,
    latency: '0μs',
    memoryUsage: '0MB',
    cpuUsage: '0%'
  }
};

// Create Express app
const app = express();
const server = http.createServer(app);
const io = socketIo(server);

// Middleware
app.use(express.json());
app.use(express.static(path.join(__dirname, '../public')));

// Routes
app.get('/', (req, res) => {
  res.sendFile(path.join(__dirname, '../public/index.html'));
});

app.get('/ar', (req, res) => {
  res.sendFile(path.join(__dirname, '../public/ar.html'));
});

// API Routes
app.post('/api/hft/start', (req, res) => {
  systemState.running = true;
  systemState.uptime = '0h 1m';
  systemState.lastUpdate = new Date().toISOString();
  
  // Simulate processing
  setInterval(() => {
    systemState.metrics.eventsProcessed += Math.floor(Math.random() * 1000);
    systemState.metrics.latency = `${Math.floor(Math.random() * 50)}μs`;
    systemState.metrics.memoryUsage = `${Math.floor(Math.random() * 100)}MB`;
    systemState.metrics.cpuUsage = `${Math.floor(Math.random() * 30)}%`;
    
    // Emit real-time updates
    io.emit('systemUpdate', systemState);
  }, 1000);
  
  res.json({ 
    success: true, 
    message: 'HFT engine started successfully',
    timestamp: new Date().toISOString()
  });
});

app.post('/api/hft/stop', (req, res) => {
  systemState.running = false;
  systemState.uptime = '0h 0m';
  
  res.json({ 
    success: true, 
    message: 'HFT engine stopped successfully',
    timestamp: new Date().toISOString()
  });
});

app.get('/api/hft/status', (req, res) => {
  res.json({ 
    success: true, 
    status: systemState
  });
});

// Socket.IO for real-time updates
io.on('connection', (socket) => {
  console.log('Client connected');
  socket.emit('systemUpdate', systemState);
  
  socket.on('disconnect', () => {
    console.log('Client disconnected');
  });
});

// Start server
const PORT = process.env.PORT || 3000;
server.listen(PORT, () => {
  console.log(`Project AEGIS Standalone Server running on port ${PORT}`);
  console.log(`Access dashboard at: http://localhost:${PORT}`);
});