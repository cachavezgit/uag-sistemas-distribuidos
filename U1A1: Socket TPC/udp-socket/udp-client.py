import socket

HOST = '127.0.0.1'
PORT = 9999

# Creación del socket UDP
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

mensaje = '¡¡¡HOLA MUNDO!!!'
sock.sendto(mensaje.encode(), (HOST, PORT))  # Envía sin establecer conexión

respuesta, _ = sock.recvfrom(1024)
print(f'[UDP] Respuesta del servidor: {respuesta.decode()}')

sock.close()
