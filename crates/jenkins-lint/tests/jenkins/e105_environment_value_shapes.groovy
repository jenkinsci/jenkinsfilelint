pipeline {
    agent any
    environment {
        A = credentials('a') ?: ''
        B = (env.JOB_NAME =~ /(\w+)/)[0][1]
        C =
        D = 'd'
    }
    stages {
        stage('Build') {
            steps {
                echo 'x'
            }
        }
    }
}
