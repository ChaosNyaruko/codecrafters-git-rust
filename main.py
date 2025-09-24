import os
import zlib
import sys
import struct

def eprint(*args, **kwargs):
    print(*args, file=sys.stderr, **kwargs)

def decompress_file(file_path):
    """
    Reads a file containing zlib-compressed data and decompresses it.

    Args:
        file_path (str): The path to the compressed file.

    Returns:
        bytes: The decompressed data.
    """
    try:
        with open(file_path, 'rb') as f:
            compressed_data = f.read()
        version = struct.unpack("!i", compressed_data[12:16])
        object_num = struct.unpack("!i", compressed_data[16:20])
        print("version",version)
        print("num", object_num)
        compressed_data = compressed_data[20:]
        for k in range(object_num[0]):
            i = 0
            type_ = (compressed_data[i]>>4)&0x07
            size = compressed_data[i]&0x0F
            shift = 4
            while compressed_data[i] & 0x80:
                b = compressed_data[i+1]
                size |= (b &0x7F) << shift
                i+=1
                shift += 7
            print(k, "type", type_)
            print(k, "size", size)
            if type_ == 1 or type_ == 2 or type_ == 3 or type_ == 4: # real object
                data = compressed_data[i+1:]
                dobj = zlib.decompressobj()
                decompressed_data = dobj.decompress(data)
                assert len(decompressed_data) == size, "the sizes of the decompressed_data should match"
                print(k, "commit content", decompressed_data)
                compressed_data = dobj.unused_data
            elif type_ == 7:
                base = compressed_data[i+1:i+1+20].hex()
                print(base)
            else:
                eprint("unsupported", type_)
                exit(69)


    except FileNotFoundError:
        print(f"Error: File not found at {file_path}")
        return None
    except zlib.error as e:
        print(f"Zlib decompression error: {e}")
        return None

decompressed_content = decompress_file("server.log")
print(decompressed_content)
