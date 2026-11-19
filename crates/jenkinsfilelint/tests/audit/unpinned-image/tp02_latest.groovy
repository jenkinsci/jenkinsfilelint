pipeline {
    agent none
    stages {
        stage('Build') {
            agent { docker 'node:latest' }
            steps {
                sh 'npm ci'
            }
        }
        stage('Lint') {
            agent { docker { image 'ghcr.io/example/linter:latest' } }
            steps {
                sh 'lint .'
            }
        }
    }
}
