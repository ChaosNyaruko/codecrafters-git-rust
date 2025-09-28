import os
import zlib
import sys
import struct
import hashlib

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
    idx = dict()
    try:
        with open(file_path, 'rb') as f:
            compressed_data = f.read()
            bak = compressed_data
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
                tname = 'commit'
                match type_:
                    case 1:
                        tname = "commit"
                    case 2:
                        tname = "tree"
                    case 3:
                        tname = "blob"
                    case 4:
                        tname = "tag"
                commit = tname.encode() + b' ' +  str(len(decompressed_data)).encode() + bytes([0]) +  decompressed_data
                obj = hashlib.sha1(commit).hexdigest()
                print("obj",  tname, obj, decompressed_data[:1000])
                idx[obj] = (tname, decompressed_data)
                compressed_data = dobj.unused_data
            elif type_ == 7:
                base = compressed_data[i+1:i+1+20].hex()
                if base not in idx:
                    # maybe the diff applied blob should also be 
                    eprint("base", base, "not found")
                    exit(69)
                t, source = idx[base]
                me = []
                dobj = zlib.decompressobj()
                data = compressed_data[i+1+20:]
                decompressed_data = dobj.decompress(data)
                assert len(decompressed_data) == size, "the sizes of the decompressed_data should match"
                j = 0
                src_size = decompressed_data[j] & 0x7F
                shift = 7
                while decompressed_data[j] & 0x80:
                    b = decompressed_data[j+1]
                    src_size |= (b &0x7F) << shift
                    j+=1
                    shift += 7
                print("src sz", src_size)
                j += 1
                dst_size = decompressed_data[j] & 0x7F
                shift = 7
                while decompressed_data[j] & 0x80:
                    b = decompressed_data[j+1]
                    dst_size |= (b &0x7F) << shift
                    j+=1
                    shift += 7
                print("dst sz", dst_size)
                # assert len(source) == src_size
                j += 1
                while j < len(decompressed_data):
                    ins = "COPY" if decompressed_data[j]&0x80 != 0 else "ADD"
                    if ins == "COPY":
                        size_to_copy = (decompressed_data[j]>>4)&0b0111
                        s1 = size_to_copy&0b001
                        s2 = size_to_copy&0b010
                        s3 = size_to_copy&0b100
                        offset_to_copy = (decompressed_data[j])&0b1111
                        of1 = offset_to_copy&0b0001
                        of2 = offset_to_copy&0b0010
                        of3 = offset_to_copy&0b0100
                        of4 = offset_to_copy&0b1000
                        print(s1, s2, s3, of1, of2, of3, of4)
                        j += 1
                        offset_ = 0
                        if of1:
                            offset_ |= decompressed_data[j]
                            j += 1
                        if of2:
                            offset_ |= decompressed_data[j] << 8
                            j += 1
                        if of3:
                            offset_ |= decompressed_data[j] << 16
                            j += 1
                        if of4:
                            offset_ |= decompressed_data[j] << 24
                            j += 1
                        print("copy offset", offset_)

                        size_ = 0
                        if s1:
                            size_ |= decompressed_data[j]
                            j += 1
                        if s2:
                            size_ |= decompressed_data[j] << 8
                            j += 1
                        if s3:
                            size_ |= decompressed_data[j] << 16
                            j += 1

                        print("copy size", size_)
                        me.extend(source[offset_:offset_+size_])
                    else: # ins = ADD
                        add_size = decompressed_data[j] & 0x7F
                        j += 1
                        added = decompressed_data[j:j+add_size]
                        j += add_size
                        me.extend(added)
                        print("ADD",  len(added), (added))
                print("j ends at", j)
                assert j == size,  "j should end at exactly where the 'size' is"
                # total copied size + total added size == dst size
                assert len(me) == dst_size
                commit = t.encode() + b' ' +  str(len(me)).encode() + bytes([0]) +  bytes(me)
                new_obj = hashlib.sha1(commit).hexdigest()
                idx[new_obj] = (t, me)
                compressed_data = dobj.unused_data
            else:
                eprint("unsupported", type_)
                exit(69)
        hash_object = hashlib.sha1(bak[8:-20])
        pbHash = hash_object.hexdigest()
        print(idx.keys(), len(idx.keys()))
        assert len(idx.keys()) == object_num[0], "objs should all be in idx"
        assert compressed_data.hex() == pbHash, "checksum not match"


    except FileNotFoundError:
        print(f"Error: File not found at {file_path}")
        return None
    except zlib.error as e:
        print(f"Zlib decompression error: {e}")
        return None

decompressed_content = decompress_file("server.log")
print(decompressed_content)
