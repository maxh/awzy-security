{{- define "awzy.image" -}}
{{- $tag := .root.Values.image.tag | default .root.Chart.AppVersion -}}
{{- required "image.registry is required" .root.Values.image.registry -}}/{{ .name }}:{{ $tag }}
{{- end -}}

{{- /* A third-party image mirrored into the release registry and signed (ADR 0057); `ref` is a
<name>:<tag> from deploy/release/third-party-images.txt. */ -}}
{{- define "awzy.thirdPartyImage" -}}
{{- required "image.registry is required" .root.Values.image.registry -}}/third-party/{{ .ref }}
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

{{/* A Deployment plus ClusterIP Service on 8080 for one awzy service, rolled out without
downtime (ADR 0072): a new pod is ready before an old one goes, an old one drains before it stops,
and with more than one replica they spread across nodes and zones and leave one at a time. */}}
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
  strategy:
    type: RollingUpdate
    rollingUpdate: { maxUnavailable: 0, maxSurge: 1 }
  selector:
    matchLabels: {{- include "awzy.selector" . | nindent 6 }}
  template:
    metadata:
      labels: {{- include "awzy.labels" . | nindent 8 }}
    spec:
      serviceAccountName: {{ .serviceAccount | default "awzy" }}
      # Covers the shutdown delay plus time to finish in-flight requests.
      terminationGracePeriodSeconds: {{ add $root.Values.shutdownDelaySeconds 20 }}
      {{- if gt (int .values.replicas) 1 }}
      topologySpreadConstraints:
        - maxSkew: 1
          topologyKey: kubernetes.io/hostname
          whenUnsatisfiable: ScheduleAnyway
          labelSelector:
            matchLabels: {{- include "awzy.selector" . | nindent 14 }}
        - maxSkew: 1
          topologyKey: topology.kubernetes.io/zone
          whenUnsatisfiable: ScheduleAnyway
          labelSelector:
            matchLabels: {{- include "awzy.selector" . | nindent 14 }}
      {{- end }}
      {{- with .tokens }}
      # Service tokens this workload sends, one per service it calls (ADR 0055).
      volumes:
        - name: awzy-service-tokens
          projected:
            sources:
              {{- range . }}
              - serviceAccountToken: { audience: {{ . }}, expirationSeconds: 600, path: {{ . }} }
              {{- end }}
      {{- end }}
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
            # Not ready on SIGTERM, but serving this long while traffic moves elsewhere.
            - { name: AWZY_SHUTDOWN_DELAY_SECS, value: {{ $root.Values.shutdownDelaySeconds | quote }} }
            # The commit this chart was released from (appVersion, set by publish-chart.sh).
            - { name: AWZY_RELEASE_VERSION, value: {{ $root.Chart.AppVersion | quote }} }
            {{- with .env }}
            {{- toYaml . | nindent 12 }}
            {{- end }}
          {{- with .tokens }}
          volumeMounts:
            - { name: awzy-service-tokens, mountPath: /var/run/secrets/awzy/tokens, readOnly: true }
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
{{- if gt (int .values.replicas) 1 }}
---
# A node drain takes one replica at a time.
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: {{ .name }}
  namespace: {{ .namespace }}
  labels: {{- include "awzy.labels" . | nindent 4 }}
spec:
  maxUnavailable: 1
  selector:
    matchLabels: {{- include "awzy.selector" . | nindent 6 }}
{{- end }}
{{- end -}}

{{- define "awzy.demoIdpIssuer" -}}
https://{{ required "clientPortal.hostname is required for demoIdp" .Values.clientPortal.hostname }}/idp
{{- end -}}
