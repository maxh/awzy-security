{{- define "awzy.image" -}}
{{- $tag := .root.Values.image.tag | default .root.Chart.AppVersion -}}
{{- required "image.registry is required" .root.Values.image.registry -}}/{{ .name }}:{{ $tag }}
{{- end -}}

{{- define "awzy.labels" -}}
app.kubernetes.io/name: {{ .name }}
app.kubernetes.io/part-of: awzy
app.kubernetes.io/version: {{ .root.Values.image.tag | default .root.Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .root.Release.Service }}
{{- end -}}

{{- define "awzy.selector" -}}
app.kubernetes.io/name: {{ .name }}
{{- end -}}

{{/* A Deployment plus ClusterIP Service on 8080 for one awzy service. */}}
{{- define "awzy.service" -}}
{{- $root := .root -}}
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ .name }}
  namespace: {{ .namespace }}
  labels: {{- include "awzy.labels" . | nindent 4 }}
spec:
  replicas: {{ .values.replicas }}
  selector:
    matchLabels: {{- include "awzy.selector" . | nindent 6 }}
  template:
    metadata:
      labels: {{- include "awzy.labels" . | nindent 8 }}
    spec:
      serviceAccountName: {{ .serviceAccount | default "awzy" }}
      {{- with .root.Values.image.pullSecret }}
      imagePullSecrets: [{ name: {{ . }} }]
      {{- end }}
      securityContext:
        runAsNonRoot: true
        seccompProfile: { type: RuntimeDefault }
      containers:
        - name: {{ .name }}
          image: {{ include "awzy.image" . }}
          ports:
            - { name: http, containerPort: 8080 }
          env:
            - { name: AWZY_LOG_FORMAT, value: {{ $root.Values.logFormat | quote }} }
            # The commit this chart was released from (appVersion, set by publish-chart.sh).
            - { name: AWZY_RELEASE_VERSION, value: {{ $root.Chart.AppVersion | quote }} }
            {{- with .env }}
            {{- toYaml . | nindent 12 }}
            {{- end }}
          readinessProbe:
            httpGet: { path: /readyz, port: http }
          livenessProbe:
            httpGet: { path: /healthz, port: http }
          resources: {{- toYaml .values.resources | nindent 12 }}
          securityContext:
            allowPrivilegeEscalation: false
            readOnlyRootFilesystem: true
            capabilities: { drop: [ALL] }
---
apiVersion: v1
kind: Service
metadata:
  name: {{ .name }}
  namespace: {{ .namespace }}
  labels: {{- include "awzy.labels" . | nindent 4 }}
spec:
  selector: {{- include "awzy.selector" . | nindent 4 }}
  ports:
    - { name: http, port: 8080, targetPort: http }
{{- end -}}

{{- define "awzy.demoIdpIssuer" -}}
https://{{ required "clientPortal.hostname is required for demoIdp" .Values.clientPortal.hostname }}/idp
{{- end -}}
