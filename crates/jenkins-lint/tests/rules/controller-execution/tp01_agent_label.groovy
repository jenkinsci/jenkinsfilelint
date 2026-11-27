pipeline {
    agent { label 'built-in' }
    stages {
        stage('Build') {
            steps {
                sh 'make'
            }
        }
    }
}
