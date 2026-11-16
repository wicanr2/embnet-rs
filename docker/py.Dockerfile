# embnet-py：pycyphal 對照組與 test vector 產生器。版本選擇見 docs/plans/004 第二節。
FROM python:3.13.18-slim-trixie@sha256:e48f59cdba69a3cbd9ed2cf0f348d90d45a8d350d98134cf1070bc106ee9ed90

RUN pip install --no-cache-dir pycyphal==1.27.1
