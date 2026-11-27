library('x@main') _
pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                echo 'x'
            }
        }
    }
}
