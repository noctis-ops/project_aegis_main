const fs = require('fs');
const path = require('path');

console.log('🔍 Project AEGIS - System Check');
console.log('================================');

// Check Node.js version
console.log('✅ Node.js Version:', process.version);

// Check required files
const requiredFiles = [
    '../config/settings.json',
    './package.json',
    '../public/index.html',
    '../public/ar.html'
];

let allFilesExist = true;
requiredFiles.forEach(file => {
    const fullPath = path.join(__dirname, file);
    if (fs.existsSync(fullPath)) {
        console.log('✅ Found:', file);
    } else {
        console.log('❌ Missing:', file);
        allFilesExist = false;
    }
});

// Check if node_modules exists
const nodeModulesPath = path.join(__dirname, 'node_modules');
if (fs.existsSync(nodeModulesPath)) {
    console.log('✅ Dependencies installed');
} else {
    console.log('⚠️  Dependencies not installed (will install automatically)');
}

// Check system resources
const os = require('os');
const totalMem = Math.round(os.totalmem() / (1024 * 1024 * 1024));
const freeMem = Math.round(os.freemem() / (1024 * 1024 * 1024));

console.log(`✅ System Memory: ${totalMem}GB Total, ${freeMem}GB Free`);

// Check platform
console.log('✅ Platform:', process.platform, os.arch());

console.log('\n🎉 System check completed!');
if (allFilesExist) {
    console.log('🚀 Ready to start Project AEGIS!');
} else {
    console.log('⚠️  Some files are missing. Please extract all files correctly.');
}

module.exports = { allFilesExist };