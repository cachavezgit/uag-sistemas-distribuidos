import socket

HOST = '127.0.0.1'
PORT = 8888

# Creación del socket TCP
sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
sock.connect((HOST, PORT))  # Inicia el handshake de 3 vías con el servidor

mensaje = '¡¡¡HOLA MUNDO!!!'
sock.sendall(mensaje.encode())

respuesta = sock.recv(1024)
print(f'[TCP] Respuesta del servidor: {respuesta.decode()}')

sock.close()
