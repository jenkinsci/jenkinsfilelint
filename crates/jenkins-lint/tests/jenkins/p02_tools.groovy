pipeline {
    agent any
    tools {
        maven 'M3'
        jdk 'jdk17'
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
