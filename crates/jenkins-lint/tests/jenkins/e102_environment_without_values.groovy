pipeline {
    agent any
    environment {
        def DNS
    }
    stages {
        stage('Build') {
            steps {
                echo 'x'
            }
        }
    }
}
