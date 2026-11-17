import ctypes, sys
x = ctypes.CDLL('libX11.so.6')
x.XOpenDisplay.restype = ctypes.c_void_p
x.XDefaultRootWindow.restype = ctypes.c_ulong
d = x.XOpenDisplay(None)
root = x.XDefaultRootWindow(ctypes.c_void_p(d))
r, p = ctypes.c_ulong(), ctypes.c_ulong()
kids = ctypes.POINTER(ctypes.c_ulong)(); n = ctypes.c_uint()
x.XQueryTree(ctypes.c_void_p(d), ctypes.c_ulong(root), ctypes.byref(r), ctypes.byref(p), ctypes.byref(kids), ctypes.byref(n))
w, h = int(sys.argv[1]), int(sys.argv[2])
for i in range(n.value):
    x.XMoveResizeWindow(ctypes.c_void_p(d), ctypes.c_ulong(kids[i]), 0, 0, w, h)
x.XFlush(ctypes.c_void_p(d))
