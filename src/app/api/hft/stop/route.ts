import { NextResponse } from 'next/server';

export async function POST(request: Request) {
  try {
    // In a real implementation, this would stop the Rust HFT engine
    // For now, we'll just simulate the response
    
    // Log the request
    console.log('Stopping HFT engine');
    
    // Simulate stopping the engine
    // In practice, this would terminate the Rust process or communicate via IPC
    
    // Check Accept-Language header for Arabic support
    const acceptLanguage = request.headers.get('accept-language') || '';
    const isArabic = acceptLanguage.includes('ar');
    
    return NextResponse.json({ 
      success: true, 
      message: isArabic 
        ? 'تم إيقاف محرك HFT بنجاح'
        : 'HFT engine stopped successfully',
      timestamp: new Date().toISOString()
    });
  } catch (error) {
    console.error('Error stopping HFT engine:', error);
    
    // Check Accept-Language header for Arabic support
    const acceptLanguage = request.headers.get('accept-language') || '';
    const isArabic = acceptLanguage.includes('ar');
    
    return NextResponse.json({ 
      success: false, 
      error: isArabic 
        ? 'فشل في إيقاف محرك HFT'
        : 'Failed to stop HFT engine'
    }, { status: 500 });
  }
}