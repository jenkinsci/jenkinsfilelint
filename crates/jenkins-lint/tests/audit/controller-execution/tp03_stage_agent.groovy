pipeline {
    agent none
    stages {
        stage('Build') {
            agent { label 'linux' }
            steps {
                sh 'make'
            }
        }
        stage('Publish') {
            agent { label "built-in" }
            steps {
                sh 'make publish'
            }
        }
    }
}
