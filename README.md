services:
  - type: web
    name: noicetom-backend
    runtime: docker
    repo: nomadtonics/noicetom
    dockerfilePath: ./Dockerfile.backend
    dockerContext: .
    autoDeploy: true
    plan: free
    envVars:
      - key: PORT
        value: 3000
      - key: DATABASE_URL
        fromDatabase:
          name: noicetom-db
          property: connectionString

  - type: web
    name: noicetom-frontend
    runtime: docker
    repo: nomadtonics/noicetom
    dockerfilePath: ./Dockerfile.frontend
    dockerContext: .
    autoDeploy: true
    plan: free
    envVars:
      - key: BACKEND_URL
        value: https://noicetom-backend.onrender.com

# Render-managed Postgres database

databases:
  - name: noicetom-db
    databaseName: noicetom
    user: noicetom
    plan: free
