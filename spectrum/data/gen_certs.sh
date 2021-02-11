#!/usr/bin/env bash
openssl req -nodes \
          -x509 \
          -days 3650 \
          -newkey rsa:4096 \
          -keyout ca.key \
          -out ca.crt \
          -sha256 \
          -batch \
          -subj "/CN=Spectrum RSA CA"
openssl req -nodes \
          -newkey rsa:2048 \
          -keyout server.key \
          -out server.req \
          -sha256 \
          -batch \
          -subj "/CN=spectrum.example.com"
openssl x509 -req \
    -in server.req \
    -out server.crt \
    -CA ca.crt \
    -CAkey ca.key \
    -sha256 \
    -days 2000 \
    -set_serial 456 \
    -extensions v3_end \
    -extfile openssl.cnf
