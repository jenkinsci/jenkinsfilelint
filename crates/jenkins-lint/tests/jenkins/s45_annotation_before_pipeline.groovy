@Library('lib')
pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                echo 'x'
            }
        }
    }
}
