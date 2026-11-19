pipeline {
    agent { label 'linux && docker' }
    stages {
        stage('Build') {
            agent any
            steps {
                sh 'make'
            }
        }
        stage('Test') {
            agent {
                kubernetes {
                    yamlFile 'pod.yaml'
                }
            }
            steps {
                sh 'make test'
            }
        }
    }
}

node {
    sh 'make'
}

node('linux') {
    sh 'make'
}
