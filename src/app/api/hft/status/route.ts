import { NextResponse } from 'next/server';

export async function GET(request: Request) {
  try {
    // In a real implementation, this would check the status of the Rust HFT engine
    // For now, we'll just simulate the response
    
    // Simulate some status data
    const statusData = {
      running: true,
      symbols: ['BTCUSDT', 'ETHUSDT'],
      uptime: '2h 15m',
      lastUpdate: new Date().toISOString(),
      metrics: {
        eventsProcessed: 1250000,
        latency: '12μs',
        memoryUsage: '45MB',
        cpuUsage: '12%'
      }
    };
    
    return NextResponse.json({ 
      success: true, 
      status: statusData
    });
  } catch (error) {
    console.error('Error getting HFT engine status:', error);
    
    // Check Accept-Language header for Arabic support
    const acceptLanguage = request.headers.get('accept-language') || '';
    const isArabic = acceptLanguage.includes('ar');
    
    return NextResponse.json({ 
      success: false, 
      error: isArabic 
        ? 'فشل في الحصول على حالة محرك HFT'
        : 'Failed to get HFT engine status'
    }, { status: 500 });
  }
}